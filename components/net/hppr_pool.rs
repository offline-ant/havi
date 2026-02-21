/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Async HPPR connection pool using hppr-client.
//!
//! Pools TCP/Unix connections by (via, signer). Idle connections are evicted after 60s.
//! Up to 4 idle connections per pool key.
//!
//! QUIB connections use a persistent base connection per (via, signer).
//! Each checkout forks a fresh bi-stream from the base connection.

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hppr_client::env_target::{ViaSpec, TransportScheme, parse_via};
use hppr_client::{
    AnyConnection, HpprError, HpprRequest as IoRequest, ResponseKind, Result, Signer,
    connect_quib_async, spawn_connection,
};
use parking_lot::Mutex;

/// Pool key: (via display string, signer).
type PoolKey = (String, Signer);

/// Idle connection with timestamp for timeout tracking.
struct IdleConnection {
    conn: AnyConnection,
    last_used: Instant,
}

/// Connection wrapper that returns to pool on drop.
pub struct PooledConnection {
    conn: Option<AnyConnection>,
    pool: Arc<HpprAsyncPool>,
    key_string: String,
    signer: Signer,
}

impl PooledConnection {
    pub fn connection(&self) -> &AnyConnection {
        self.conn.as_ref().expect("connection already taken")
    }

    pub fn connection_mut(&mut self) -> &mut AnyConnection {
        self.conn.as_mut().expect("connection already taken")
    }

    /// Take the connection without returning it to the pool.
    pub fn take(mut self) -> AnyConnection {
        self.conn.take().expect("connection already taken")
    }

    /// Invalidate this connection (don't return to pool).
    pub fn invalidate(&mut self) {
        self.conn.take();
        self.pool.invalidate(&self.key_string);
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.release(&self.key_string, &self.signer, conn);
        }
    }
}

/// Async HPPR connection pool.
pub struct HpprAsyncPool {
    connections: Mutex<HashMap<PoolKey, Vec<IdleConnection>>>,
    quib_bases: Mutex<HashMap<PoolKey, AnyConnection>>,
    max_idle: usize,
    idle_timeout: Duration,
}

impl HpprAsyncPool {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            quib_bases: Mutex::new(HashMap::new()),
            max_idle: 4,
            idle_timeout: Duration::from_secs(60),
        }
    }

    /// Get a connection from pool or create a new one.
    ///
    /// Transport dispatch is based on ViaSpec:
    /// - `scheme: Some(Quib)` → QUIB (multiplexed, persistent base)
    /// - `scheme: Some(Tcp)` → TCP
    /// - `scheme: Some(Ws)` → WebSocket (treated as TCP connection)
    /// - `scheme: None` → auto-negotiate: connect TCP, check HELLO Transport headers
    /// - `ViaSpec::Unix` → Unix socket
    pub async fn get(&self, via: &ViaSpec, signer: Signer) -> Result<AnyConnection> {
        let key_string = via.to_string();

        match via {
            ViaSpec::Net { host, port, scheme } => {
                let addr = resolve_host_port(host, *port).await?;

                match scheme {
                    Some(TransportScheme::Quib) => {
                        self.get_quib(&key_string, addr, signer).await
                    }
                    Some(TransportScheme::Tcp) => {
                        self.get_tcp(&key_string, addr, signer).await
                    }
                    Some(TransportScheme::Ws) => {
                        // WS connections use spawn_ws_connection which returns
                        // a TCP-compatible connection handle.
                        if let Some(conn) = self.try_idle(&key_string, &signer) {
                            return Ok(conn);
                        }
                        let conn = hppr_client::spawn_ws_connection(
                            host.clone(), *port, signer,
                        ).await?;
                        Ok(AnyConnection::Tcp(conn))
                    }
                    Some(TransportScheme::Udp) => {
                        Ok(AnyConnection::Udp(
                            hppr_client::connect_udp_stateless(addr).await?,
                        ))
                    }
                    None => {
                        // Auto-negotiate: try idle first, then connect TCP and
                        // check HELLO Transport headers for QUIB upgrade.
                        if let Some(conn) = self.try_idle(&key_string, &signer) {
                            return Ok(conn);
                        }
                        self.connect_auto(addr, signer).await
                    }
                }
            }
            #[cfg(unix)]
            ViaSpec::Unix { path } => {
                let key_string = via.to_string();
                if let Some(conn) = self.try_idle(&key_string, &signer) {
                    return Ok(conn);
                }
                let conn = hppr_client::spawn_unix_connection(
                    path.clone(), signer,
                ).await?;
                Ok(AnyConnection::Unix { conn, path: path.clone() })
            }
            #[cfg(not(unix))]
            ViaSpec::Unix { path } => {
                Err(HpprError::Connection(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!("unix sockets not supported: {}", path.display()),
                )))
            }
            ViaSpec::Unknown { scheme, .. } => {
                Err(HpprError::Connection(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!("unsupported transport scheme: {}", scheme),
                )))
            }
        }
    }

    /// Try to get an idle TCP/Unix connection from the pool.
    fn try_idle(&self, key_string: &str, signer: &Signer) -> Option<AnyConnection> {
        let key = (key_string.to_string(), signer.clone());
        let mut pool = self.connections.lock();
        if let Some(conns) = pool.get_mut(&key) {
            while let Some(idle) = conns.pop() {
                if idle.last_used.elapsed() < self.idle_timeout {
                    return Some(idle.conn);
                }
            }
        }
        None
    }

    /// Get a TCP connection from pool or create a new one.
    async fn get_tcp(&self, key_string: &str, addr: SocketAddr, signer: Signer) -> Result<AnyConnection> {
        if let Some(conn) = self.try_idle(key_string, &signer) {
            return Ok(conn);
        }
        Ok(AnyConnection::Tcp(spawn_connection(addr, signer).await?))
    }

    /// Get a QUIB connection by forking from a persistent base connection.
    async fn get_quib(&self, key_string: &str, addr: SocketAddr, signer: Signer) -> Result<AnyConnection> {
        let key = (key_string.to_string(), signer.clone());

        // Take base out of map while we await fork/connect.
        let base = { self.quib_bases.lock().remove(&key) };

        let (base, forked) = match base {
            Some(base @ AnyConnection::Quib(_)) => match base.fork().await {
                Ok(forked) => (base, forked),
                Err(_) => {
                    let base = AnyConnection::Quib(Box::new(connect_quib_async(addr, signer).await?));
                    let forked = base.fork().await?;
                    (base, forked)
                },
            },
            _ => {
                let base = AnyConnection::Quib(Box::new(connect_quib_async(addr, signer).await?));
                let forked = base.fork().await?;
                (base, forked)
            },
        };

        self.quib_bases.lock().insert(key, base);
        Ok(forked)
    }

    /// Auto-negotiate transport: connect TCP, send HELLO, upgrade to QUIB if available.
    async fn connect_auto(&self, addr: SocketAddr, signer: Signer) -> Result<AnyConnection> {
        let conn = spawn_connection(addr, signer.clone()).await?;
        let resp = conn.send(IoRequest::Hello).await?;
        if let ResponseKind::Greeting(greeting) = &resp.kind {
            let host_str = addr.ip().to_string();
            for via in greeting.transports_for_host(&host_str) {
                match via {
                    ViaSpec::Net { scheme: Some(TransportScheme::Tcp), .. } => {
                        return Ok(AnyConnection::Tcp(conn));
                    }
                    ViaSpec::Net { scheme: Some(TransportScheme::Quib), ref host, port, .. } => {
                        if let Ok(quib_addr) = resolve_host_port(host, port).await {
                            match connect_quib_async(quib_addr, signer.clone()).await {
                                Ok(quib) => return Ok(AnyConnection::Quib(Box::new(quib))),
                                Err(_) => continue,
                            }
                        }
                    }
                    _ => continue,
                }
            }
        }
        Ok(AnyConnection::Tcp(conn))
    }

    /// Get a pooled connection that automatically returns to the pool on drop.
    pub async fn get_pooled(
        pool: Arc<HpprAsyncPool>,
        via: &ViaSpec,
        signer: Signer,
    ) -> Result<PooledConnection> {
        let conn = pool.get(via, signer.clone()).await?;
        Ok(PooledConnection {
            conn: Some(conn),
            pool,
            key_string: via.to_string(),
            signer,
        })
    }

    /// Return a connection to the pool for reuse.
    pub fn release(&self, key_string: &str, signer: &Signer, conn: AnyConnection) {
        // QUIB uses stream multiplexing. Base connections are kept in quib_bases.
        if matches!(conn, AnyConnection::Quib(_)) {
            return;
        }

        let key = (key_string.to_string(), signer.clone());
        let mut pool = self.connections.lock();
        let conns = pool.entry(key).or_default();
        if conns.len() < self.max_idle {
            conns.push(IdleConnection {
                conn,
                last_used: Instant::now(),
            });
        }
    }

    /// Remove all cached connections for this key string (any signer).
    pub fn invalidate(&self, key_string: &str) {
        let mut pool = self.connections.lock();
        pool.retain(|(ep, _), _| ep != key_string);

        let mut quib_bases = self.quib_bases.lock();
        quib_bases.retain(|(ep, _), _| ep != key_string);
    }

    #[allow(dead_code)]
    pub fn clear(&self) {
        self.connections.lock().clear();
        self.quib_bases.lock().clear();
    }
}

impl Default for HpprAsyncPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve a ViaSpec to a SocketAddr (for loaders that manage their own connections).
///
/// Only supports Net targets. Unix targets return an error.
pub async fn resolve_via_to_addr(via: &ViaSpec) -> Result<SocketAddr> {
    match via {
        ViaSpec::Net { host, port, .. } => resolve_host_port(host, *port).await,
        #[cfg(unix)]
        ViaSpec::Unix { path } => Err(HpprError::Connection(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("cannot resolve unix socket to SocketAddr: {}", path.display()),
        ))),
        #[cfg(not(unix))]
        ViaSpec::Unix { path } => Err(HpprError::Connection(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("unix sockets not supported: {}", path.display()),
        ))),
        ViaSpec::Unknown { scheme, .. } => Err(HpprError::Connection(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("cannot resolve {} target to SocketAddr", scheme),
        ))),
    }
}

/// Resolve host:port to SocketAddr.
pub async fn resolve_host_port(host: &str, port: u16) -> Result<SocketAddr> {
    let endpoint = format!("{}:{}", host, port);
    resolve_endpoint(&endpoint).await
}

/// Resolve endpoint string to SocketAddr.
pub async fn resolve_endpoint(endpoint: &str) -> Result<SocketAddr> {
    if let Ok(addr) = endpoint.parse::<SocketAddr>() {
        return Ok(addr);
    }

    let addrs: Vec<_> = tokio::net::lookup_host(endpoint)
        .await
        .map_err(|e| HpprError::Connection(io::Error::new(
            io::ErrorKind::Other,
            format!("resolve {}: {}", endpoint, e),
        )))?
        .collect();

    addrs.into_iter().next().ok_or_else(|| {
        HpprError::Connection(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no addresses for {}", endpoint),
        ))
    })
}

/// Async HPPR state with connection pool and default target.
pub struct HpprAsyncState {
    pub pool: Arc<HpprAsyncPool>,
    /// Default repo target (home repo).
    pub default_target: ViaSpec,
}

impl HpprAsyncState {
    pub fn new(target: ViaSpec) -> Self {
        Self {
            pool: Arc::new(HpprAsyncPool::new()),
            default_target: target,
        }
    }

    pub fn from_env() -> Self {
        Self::new(hppr_client::repo_target().clone())
    }

    /// Default endpoint as a display string (for legacy interfaces).
    pub fn default_endpoint(&self) -> String {
        self.default_target.to_string()
    }

    pub async fn get_pooled(&self, via: &ViaSpec, signer: Signer) -> Result<PooledConnection> {
        HpprAsyncPool::get_pooled(Arc::clone(&self.pool), via, signer).await
    }

    /// Get a pooled connection from an endpoint string.
    ///
    /// Parses the string with `parse_via()`. Used at boundaries where endpoint
    /// arrives as a String (e.g. CoreResourceMsg::HpprOperation).
    pub async fn get_pooled_str(&self, endpoint: &str, signer: Signer) -> Result<PooledConnection> {
        let via = parse_via(endpoint).map_err(|e| {
            HpprError::Connection(io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))
        })?;
        self.get_pooled(&via, signer).await
    }
}
