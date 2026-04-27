/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared utilities and async HPPR client for hpprd operations.
//!
//! Contains:
//! - local route record helpers
//! - local route auth helpers
//! - `HpprdClientAsync`: Async client for hpprd daemon operations

use hppr_client::Packet;
use hppr_client::{
    AnyConnection, HpprRequest as IoRequest, ResponseKind, Signer, ViaSpec, parse_via,
    spawn_connection,
};
use hppr_packet::PacketType;
use tokio::sync::Mutex;

use super::credentials::{DEFAULT_RING0_NAME, DEFAULT_ROOT_TOKEN, global_credential_store};

/// Local exact-app route record information.
#[derive(Debug, Clone)]
pub struct LocalRouteAppInfo {
    pub upstream: Option<ViaSpec>,
    pub upstream_verification_key: Option<String>,
    pub content_authority: Option<String>,
}

/// Local exact-group route record information.
#[derive(Debug, Clone)]
pub struct LocalRouteGroupInfo {
    pub upstream: ViaSpec,
    pub route_authority_key: String,
    pub upstream_verification_key: Option<String>,
    pub home_app: Option<String>,
}

/// Local route auth record loaded from `//repo/route/auth/...`.
#[derive(Debug, Clone)]
pub struct RouteAuthInfo {
    pub auth: String,
}

/// App content pointer information for routed app content.
#[derive(Debug, Clone)]
pub struct ContentPointerInfo {
    pub root: String,
    pub authority: String,
}

/// Helper to get raw admin credentials from the global store, falling back to defaults.
///
/// Returns (ring1_name, token) tuple. Used to construct a Signer::ring1_adhoc.
pub fn get_admin_credentials() -> (String, String) {
    let store = global_credential_store();
    if let Some(cred) = store.get_admin() {
        (cred.ring1_name.clone(), cred.token().to_string())
    } else {
        (
            DEFAULT_RING0_NAME.to_string(),
            DEFAULT_ROOT_TOKEN.to_string(),
        )
    }
}

/// Async client for communicating with hpprd daemon.
///
/// Uses async connections via hppr_client for non-blocking operations.
/// Signer is bound at construction.
pub struct HpprdClientAsync {
    target: ViaSpec,
    signer: Signer,
    /// Cached connection handle.
    conn: Mutex<Option<AnyConnection>>,
}

impl HpprdClientAsync {
    /// Create a new async client targeting the given endpoint.
    ///
    /// Uses admin adhoc credentials by default.
    pub fn new(target: ViaSpec) -> Result<Self, String> {
        let (ring1_name, token) = get_admin_credentials();
        let signer = Signer::ring1_adhoc(&ring1_name, &token);
        Ok(Self {
            target,
            signer,
            conn: Mutex::new(None),
        })
    }

    /// Create a new async client with a specific signer.
    pub fn new_with_signer(target: ViaSpec, signer: Signer) -> Self {
        Self {
            target,
            signer,
            conn: Mutex::new(None),
        }
    }

    async fn ensure_conn<'a>(
        &'a self,
        guard: &'a mut Option<AnyConnection>,
    ) -> Result<&'a mut AnyConnection, String> {
        if guard.is_none() {
            *guard = Some(self.connect_via().await?);
        }
        Ok(guard.as_mut().unwrap())
    }

    pub fn target(&self) -> ViaSpec {
        self.target.clone()
    }

    /// Connect based on ViaSpec, with auto-negotiation for scheme: None.
    async fn connect_via(&self) -> Result<AnyConnection, String> {
        match &self.target {
            ViaSpec::Net { host, port, scheme } => {
                let addr: std::net::SocketAddr = format!("{}:{}", host, port)
                    .parse()
                    .map_err(|e| format!("invalid address '{}:{}': {}", host, port, e))?;
                match scheme {
                    Some(hppr_client::TransportScheme::Tcp) => {
                        let conn = spawn_connection(addr, self.signer.clone())
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(AnyConnection::Tcp(conn))
                    },
                    Some(hppr_client::TransportScheme::Quib) => {
                        let conn = hppr_client::connect_quib_async(addr, self.signer.clone())
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(AnyConnection::Quib(Box::new(conn)))
                    },
                    Some(hppr_client::TransportScheme::Ws) => {
                        let conn = hppr_client::spawn_ws_connection(
                            host.clone(),
                            *port,
                            self.signer.clone(),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                        Ok(AnyConnection::Ws {
                            conn,
                            host: host.clone(),
                            port: *port,
                        })
                    },
                    Some(hppr_client::TransportScheme::Udp) => {
                        let conn = hppr_client::connect_udp_stateless(addr)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(AnyConnection::Udp(conn))
                    },
                    None => {
                        // Auto-negotiate: connect TCP, check Transport headers
                        let mut conn = spawn_connection(addr, self.signer.clone())
                            .await
                            .map_err(|e| e.to_string())?;
                        let resp = conn
                            .send(IoRequest::Hello)
                            .await
                            .map_err(|e| e.to_string())?;
                        if let ResponseKind::Greeting(greeting) = &resp.kind {
                            let host_str = addr.ip().to_string();
                            for via in greeting.transports_for_host(&host_str) {
                                match via {
                                    ViaSpec::Net {
                                        scheme: Some(hppr_client::TransportScheme::Tcp),
                                        ..
                                    } => {
                                        return Ok(AnyConnection::Tcp(conn));
                                    },
                                    ViaSpec::Net {
                                        scheme: Some(hppr_client::TransportScheme::Quib),
                                        ref host,
                                        port,
                                        ..
                                    } => {
                                        let quib_addr: std::net::SocketAddr =
                                            format!("{}:{}", host, port).parse().map_err(|e| {
                                                format!(
                                                    "invalid quib address '{}:{}': {}",
                                                    host, port, e
                                                )
                                            })?;
                                        let quib = hppr_client::connect_quib_async(
                                            quib_addr,
                                            self.signer.clone(),
                                        )
                                        .await
                                        .map_err(|e| e.to_string())?;
                                        return Ok(AnyConnection::Quib(Box::new(quib)));
                                    },
                                    _ => continue,
                                }
                            }
                        }
                        Ok(AnyConnection::Tcp(conn))
                    },
                }
            },
            #[cfg(unix)]
            ViaSpec::Unix { path } => {
                let conn = hppr_client::spawn_unix_connection(path.clone(), self.signer.clone())
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(AnyConnection::Unix {
                    conn,
                    path: path.clone(),
                })
            },
            #[cfg(not(unix))]
            ViaSpec::Unix { path } => {
                Err(format!("unix sockets not supported: {}", path.display()))
            },
            ViaSpec::Unknown { scheme, .. } => {
                Err(format!("unsupported transport scheme: {}", scheme))
            },
        }
    }

    /// Send a request and return the response.
    async fn send(&self, request: IoRequest) -> Result<hppr_client::HpprResponse, String> {
        let mut guard = self.conn.lock().await;
        let conn = self.ensure_conn(&mut guard).await?;
        match conn.send(request.clone()).await {
            Ok(response) => Ok(response),
            Err(e) if e.should_close() => {
                *guard = Some(self.connect_via().await?);
                guard
                    .as_mut()
                    .unwrap()
                    .send(request)
                    .await
                    .map_err(|e| e.to_string())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Send a raw protocol request.
    pub async fn execute(&self, request: IoRequest) -> Result<hppr_client::HpprResponse, String> {
        self.send(request).await
    }

    /// Send a request and extract a Packet from the response.
    async fn request_packet(&self, request: IoRequest) -> Result<Packet, String> {
        let resp = self.send(request).await?;
        match resp.kind {
            ResponseKind::Packet(p) => Ok(p),
            _ => Err("unexpected response type: expected Packet".into()),
        }
    }

    /// Send a request and extract Lines from the response.
    async fn request_lines(&self, request: IoRequest) -> Result<String, String> {
        let resp = self.send(request).await?;
        match resp.kind {
            ResponseKind::Lines(text) => Ok(text),
            _ => Err("unexpected response type: expected Lines".into()),
        }
    }

    /// Parse a Lines response into non-empty line Vec.
    fn parse_lines(text: &str) -> Vec<String> {
        text.lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect()
    }

    /// Authenticated GET.
    pub async fn get_authenticated(
        &self,
        urc: &str,
    ) -> Result<(String, Vec<u8>), String> {
        let p = self
            .request_packet(IoRequest::Get {
                urc: urc.to_string(),
            })
            .await?;
        Ok(extract_content(&p))
    }

    /// GET returning validated Packet.
    async fn get_packet(&self, urc: &str) -> Result<Packet, String> {
        self.request_packet(IoRequest::Get {
            urc: urc.to_string(),
        })
        .await
    }

    /// GET returning validated Packet.
    pub async fn get_packet_authenticated(
        &self,
        urc: &str,
    ) -> Result<Packet, String> {
        self.get_packet(urc).await
    }

    /// LIST.
    pub async fn list(&self, urc: &str) -> Result<Vec<String>, String> {
        let text = self
            .request_lines(IoRequest::List {
                urc: urc.to_string(),
            })
            .await?;
        Ok(Self::parse_lines(&text))
    }

    /// MEMBERS.
    pub async fn members(&self, urc: &str) -> Result<Vec<String>, String> {
        let text = self
            .request_lines(IoRequest::Members {
                args: urc.to_string(),
            })
            .await?;
        Ok(Self::parse_lines(&text))
    }

    /// ADD.
    pub async fn add(&self, add_args: &[u8]) -> Result<String, String> {
        self.request_lines(IoRequest::Add {
            headers: add_args.to_vec(),
            data: None,
        })
        .await
    }

    // ========================================================================
    // Local Route Record Methods
    // ========================================================================

    pub async fn get_local_route_app(
        &self,
        group: &str,
        app: &str,
        repo_vkey: &str,
    ) -> Result<LocalRouteAppInfo, String> {
        let urc = format!("//repo/route/app/{}/{}/|/seal/{}", group, app, repo_vkey);
        let packet = self.get_packet(&urc).await?;

        let packet_type = hppr_packet::Packet::parse(packet.as_bytes().to_vec().into_boxed_slice())
            .map(|p| p.packet_type())
            .unwrap_or(PacketType::Null);
        if packet_type != PacketType::Seal {
            return Err(format!(
                "Local route app packet is not sealed (got type: {:?})",
                packet_type
            ));
        }

        let upstream = match packet.header("Upstream") {
            Some(v) => Some(
                parse_via(v).map_err(|e| format!("invalid Upstream header '{}': {}", v, e))?,
            ),
            None => None,
        };
        let upstream_verification_key = packet
            .header("Upstream-Verification-Key")
            .map(|s| s.trim().to_string())
            .filter(|v| !v.is_empty());
        let content_authority = packet
            .header("Content-Authority")
            .map(|s| s.trim().to_string())
            .filter(|v| !v.is_empty());

        Ok(LocalRouteAppInfo {
            upstream,
            upstream_verification_key,
            content_authority,
        })
    }

    pub async fn get_local_route_group(
        &self,
        group: &str,
        repo_vkey: &str,
    ) -> Result<LocalRouteGroupInfo, String> {
        let urc = format!("//repo/route/group/{}/|/seal/{}", group, repo_vkey);
        let packet = self.get_packet(&urc).await?;

        let upstream_raw = packet
            .header("Upstream")
            .ok_or("Local route group packet missing Upstream header")?;
        let upstream = parse_via(upstream_raw)
            .map_err(|e| format!("invalid Upstream header '{}': {}", upstream_raw, e))?;
        let route_authority_key = packet
            .header("Route-Authority-Key")
            .ok_or("Local route group packet missing Route-Authority-Key header")?
            .to_string();
        let upstream_verification_key = packet
            .header("Upstream-Verification-Key")
            .map(|s| s.trim().to_string())
            .filter(|v| !v.is_empty());
        let home_app = packet
            .header("Home-App")
            .map(|s| s.trim().to_string())
            .filter(|v| !v.is_empty());

        Ok(LocalRouteGroupInfo {
            upstream,
            route_authority_key,
            upstream_verification_key,
            home_app,
        })
    }

    pub async fn get_route_auth(
        &self,
        group: &str,
        app: Option<&str>,
        repo_vkey: &str,
    ) -> Result<RouteAuthInfo, String> {
        let mut urcs = Vec::with_capacity(2);
        if let Some(app) = app {
            urcs.push(format!("//repo/route/auth/{}/{}/|/seal/{}", group, app, repo_vkey));
        }
        urcs.push(format!("//repo/route/auth/{}/|/seal/{}", group, repo_vkey));

        for urc in urcs {
            let packet = match self.get_packet(&urc).await {
                Ok(packet) => packet,
                Err(_) => continue,
            };
            let auth = packet
                .header("Auth")
                .ok_or("Route auth packet missing Auth header")?
                .to_string();
            return Ok(RouteAuthInfo { auth });
        }

        Err("Route auth packet not found".to_string())
    }

    pub async fn ensure_route_auth(
        &self,
        group: &str,
        repo_vkey: &str,
    ) -> Result<RouteAuthInfo, String> {
        if let Ok(existing) = self.get_route_auth(group, None, repo_vkey).await {
            let signer = Signer::parse(&existing.auth).map_err(|e| e.to_string())?;
            match signer {
                Signer::Ring2 { group: existing_group, .. } if existing_group == group => {
                    return Ok(existing);
                }
                _ => {
                    return Err(format!(
                        "existing route auth for '{}' is not a group-bound Ring2 signer",
                        group
                    ));
                }
            }
        }

        let (signing_key, _verification_key) = generate_keypair();
        let add_args = format!(
            "Seal-By: ring0\n\
             Group: repo\n\
             App: route\n\
             Location: auth/{}\n\
             Auth: ring2:{}|{}\n",
            group, group, signing_key
        );
        self.add(add_args.as_bytes()).await?;

        self.get_route_auth(group, None, repo_vkey).await
    }

    /// Get admin identity (verification key) from host.
    ///
    /// Used for local route lookups and app-content pointer resolution.
    pub async fn get_admin_identity(&self) -> Result<String, String> {
        let urc = "//repo/admin/identity/|";
        let packet = self.get_packet(urc).await?;

        packet
            .header("Seal-By")
            .map(|s| s.to_string())
            .ok_or_else(|| "Host identity missing Seal-By header".to_string())
    }

    // ========================================================================
    // App Content Pointer Methods
    // ========================================================================

    /// Get app content pointer for a group/app from target repo.
    ///
    /// Coordinate:
    /// `//<group>/admin/deploy/<app>/|/seal/<repo-vkey>`
    pub async fn get_content_pointer(
        &self,
        group: &str,
        app: &str,
        repo_vkey: &str,
    ) -> Result<ContentPointerInfo, String> {
        let urc = format!("//{}/admin/deploy/{}/|/seal/{}", group, app, repo_vkey);
        let packet = self.get_packet(&urc).await?;

        let root = packet
            .header("Content-Root")
            .ok_or("Content pointer packet missing Content-Root header")?
            .to_string();
        let authority = packet
            .header("Content-Authority")
            .ok_or("Content pointer packet missing Content-Authority header")?
            .to_string();

        if !root.starts_with("//") {
            return Err(format!("invalid Content-Root '{}': must start with //", root));
        }
        if !(authority.starts_with("V.") && authority.ends_with(".H3")) {
            return Err(format!(
                "invalid Content-Authority '{}': must start with V. and end with .H3",
                authority
            ));
        }

        Ok(ContentPointerInfo { root, authority })
    }

}

/// Extract content-type and body from a packet.
fn extract_content(packet: &Packet) -> (String, Vec<u8>) {
    let content_type = packet
        .header("Content-Type")
        .unwrap_or("text/html")
        .to_string();
    (content_type, packet.data().to_vec())
}

/// Generate a new HSB3 keypair.
///
/// Returns (signing_key, verification_key) in HPPR format.
fn generate_keypair() -> (String, String) {
    hppr_packet::crypto::generate_signing_verifying_pair()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_keypair() {
        let (signing_key, verification_key) = generate_keypair();

        // Signing key format: &.<base64>.<hash_algo>
        assert!(
            signing_key.starts_with("&."),
            "signing key should start with '&.'"
        );
        assert!(
            signing_key.ends_with(".H3"),
            "signing key should end with '.H3'"
        );

        // Verification key format: V.<base64>.<hash_algo>
        assert!(
            verification_key.starts_with("V."),
            "verification key should start with 'V.'"
        );
        assert!(
            verification_key.ends_with(".H3"),
            "verification key should end with '.H3'"
        );
    }
}
