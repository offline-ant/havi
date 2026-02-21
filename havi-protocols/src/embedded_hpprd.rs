/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Embedded hpprd repo daemon running on a background thread.
//!
//! Provides a localhost HPPR repo daemon for storing content locally.
//! Callers pass a typed `repo_path` from the app entry point.
//!
//! Uses `RepoInstance` for instance locking to prevent multiple servers.

use std::net::TcpListener;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread;

use hpprd::instance::{create_repository, InstanceConfig, RepoInstance};
use hpprd::{BoundListener, ListenerConfig, RepoSocket, compute_transports};

/// Handle to the embedded hpprd repo daemon thread.
pub struct EmbeddedHpprd {
    _thread: thread::JoinHandle<()>,
    port: u16,
}

impl EmbeddedHpprd {
    /// Start embedded hpprd on a background thread with the given repo path.
    ///
    /// Instance locking prevents multiple repo daemons on the same repo path.
    pub fn start(repo_path: PathBuf) -> Result<Self, String> {
        let config = InstanceConfig::new(repo_path.clone());

        // Acquire instance: PID lock + TCP + Unix socket
        // Fails if another instance is already running on this repo
        let instance = RepoInstance::acquire(config).map_err(|e| e.to_string())?;

        let actual_port = instance.port();
        let repo_path_owned = instance.config().repo_path.clone();

        // Get listeners and lock, consuming instance
        let (tcp_listener, unix_listener, pid_lock) = instance.into_parts();

        let thread = thread::spawn(move || {
            // Keep PID lock alive for repo daemon lifetime
            let _lock = pid_lock;
            if let Err(e) = run_repo_daemon(&repo_path_owned, tcp_listener, unix_listener) {
                log::error!("Embedded hpprd error: {e}");
            }
        });

        Ok(Self {
            _thread: thread,
            port: actual_port,
        })
    }

    /// Get the actual bound port.
    pub fn port(&self) -> u16 {
        self.port
    }
}

/// Run the hpprd repo daemon with pre-bound listeners (blocking).
fn run_repo_daemon(
    repo_path: &PathBuf,
    tcp_listener: TcpListener,
    unix_listener: Option<UnixListener>,
) -> Result<(), String> {
    let phc = std::env::var("HPPR_PHC").ok().filter(|s| !s.is_empty());

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime error: {e}"))?;

    let repo = create_repository(repo_path, phc.as_deref(), runtime.handle())
        .map_err(|e| format!("repo error: {e}"))?;

    let port = tcp_listener.local_addr().map(|a| a.port()).unwrap_or(4777);
    let ws_port = port + 1;
    let quib_port = port.saturating_sub(1);

    let tcp_addr = tcp_listener.local_addr().map_err(|e| e.to_string())?;
    let mut listeners = vec![BoundListener::Tcp(tcp_listener, tcp_addr)];

    if let Some(unix) = unix_listener {
        let path = repo_path.join("hppr.sock");
        listeners.push(BoundListener::Unix(unix, path));
    }

    let ws_listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, ws_port))
        .map_err(|e| format!("failed to bind WebSocket on localhost:{}: {}", ws_port, e))?;
    let ws_addr = ws_listener.local_addr().map_err(|e| e.to_string())?;
    listeners.push(BoundListener::WebSocket(ws_listener, ws_addr));

    let quib_socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, quib_port))
        .map_err(|e| format!("failed to bind QUIB on localhost:{}: {}", quib_port, e))?;
    let quib_addr = quib_socket.local_addr().map_err(|e| e.to_string())?;
    listeners.push(BoundListener::Quib(quib_socket, quib_addr));

    let udp_socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .map_err(|e| format!("failed to bind UDP on localhost:{}: {}", port, e))?;
    let udp_addr = udp_socket.local_addr().map_err(|e| e.to_string())?;
    listeners.push(BoundListener::Udp(udp_socket, udp_addr));

    log::info!(
        "Embedded hpprd starting on localhost:{} (ws: {}, quib: {}, udp: {})",
        port, ws_port, quib_port, port
    );

    let transports = compute_transports(&listeners);
    let config = ListenerConfig {
        phc,
        transports,
        version: env!("CARGO_PKG_VERSION").to_string(),
        start_tai: hpprd::Tai::now(),
        ..Default::default()
    };

    let server = RepoSocket::new_shared(config, repo);

    runtime.block_on(async {
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        server
            .run_listeners(listeners, shutdown_rx)
            .await
            .map_err(|e| format!("server error: {e}"))
    })
}
