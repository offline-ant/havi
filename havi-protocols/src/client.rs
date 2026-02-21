/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared utilities and async HPPR client for hpprd operations.
//!
//! Contains:
//! - `site_ring1_name`: Generate ring1 account names from group/app
//! - `RouteInfo`: Route information extracted from route packets
//! - `HpprdClientAsync`: Async client for hpprd daemon operations

use hppr_client::Packet;
use hppr_client::env_target::ViaSpec;
use hppr_client::{
    spawn_connection, AnyConnection, HpprRequest as IoRequest, ResponseKind, Signer,
};
use hppr_packet::{acl_coord_sort_key, PacketType};
use tokio::sync::Mutex;

use crate::credentials::{
    global_credential_store, SiteCredential, DEFAULT_RING0_NAME, DEFAULT_ROOT_TOKEN,
};

// ============================================================================
// Ring1 Account Helpers
// ============================================================================

/// Generate ring1 account name for a site sandbox.
///
/// Format: `site:<group>#<app>`
///
/// Components:
/// - `site:` - prefix identifying per-site sandbox
/// - `<group>` - full group name
/// - `#` - separator (illegal in group/app names per 010-PACKETS.md:80)
/// - `<app>` - full app name
pub fn site_ring1_name(group: &str, app: &str) -> String {
    format!("site:{}#{}", group, app)
}

/// Route information extracted from a route packet.
///
/// Route packets are sealed packets at
/// `//repo/admin/route/<group>/<app>/|/seal/<repo-vkey>` containing endpoint
/// information for remote HPPR servers.
#[derive(Debug, Clone)]
pub struct RouteInfo {
    /// Upstream: parsed via target of the remote repo.
    pub upstream: Option<ViaSpec>,
    /// Upstream-Verification-Key: repo's verification key from HELLO greeting.
    /// Used for ring2 matching and upstream identity verification on reconnect.
    pub upstream_verification_key: Option<String>,
}

/// Route key information for per-group Ring2 authentication.
#[derive(Debug, Clone)]
pub struct RouteKeyInfo {
    pub signing_key: String,
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

    async fn get_conn(&self) -> Result<AnyConnection, String> {
        let mut guard = self.conn.lock().await;
        if let Some(conn) = guard.as_ref() {
            return Ok(conn.fork().await.map_err(|e| e.to_string())?);
        }
        let any = self.connect_via().await?;
        *guard = Some(any.fork().await.map_err(|e| e.to_string())?);
        Ok(any)
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
                    }
                    Some(hppr_client::TransportScheme::Quib) => {
                        let conn = hppr_client::connect_quib_async(addr, self.signer.clone())
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(AnyConnection::Quib(Box::new(conn)))
                    }
                    Some(hppr_client::TransportScheme::Ws) => {
                        let conn = hppr_client::spawn_ws_connection(host.clone(), *port, self.signer.clone())
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(AnyConnection::Tcp(conn))
                    }
                    Some(hppr_client::TransportScheme::Udp) => {
                        let conn = hppr_client::connect_udp_stateless(addr)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(AnyConnection::Udp(conn))
                    }
                    None => {
                        // Auto-negotiate: connect TCP, check Transport headers
                        let conn = spawn_connection(addr, self.signer.clone())
                            .await
                            .map_err(|e| e.to_string())?;
                        let resp = conn.send(IoRequest::Hello).await.map_err(|e| e.to_string())?;
                        if let ResponseKind::Greeting(greeting) = &resp.kind {
                            let host_str = addr.ip().to_string();
                            for via in greeting.transports_for_host(&host_str) {
                                match via {
                                    ViaSpec::Net { scheme: Some(hppr_client::TransportScheme::Tcp), .. } => {
                                        return Ok(AnyConnection::Tcp(conn));
                                    }
                                    ViaSpec::Net { scheme: Some(hppr_client::TransportScheme::Quib), ref host, port, .. } => {
                                        let quib_addr: std::net::SocketAddr = format!("{}:{}", host, port)
                                            .parse()
                                            .map_err(|e| format!("invalid quib address '{}:{}': {}", host, port, e))?;
                                        let quib = hppr_client::connect_quib_async(quib_addr, self.signer.clone())
                                            .await
                                            .map_err(|e| e.to_string())?;
                                        return Ok(AnyConnection::Quib(Box::new(quib)));
                                    }
                                    _ => continue,
                                }
                            }
                        }
                        Ok(AnyConnection::Tcp(conn))
                    }
                }
            }
            #[cfg(unix)]
            ViaSpec::Unix { path } => {
                let conn = hppr_client::spawn_unix_connection(path.clone(), self.signer.clone())
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(AnyConnection::Unix { conn, path: path.clone() })
            }
            #[cfg(not(unix))]
            ViaSpec::Unix { path } => {
                Err(format!("unix sockets not supported: {}", path.display()))
            }
            ViaSpec::Unknown { scheme, .. } => {
                Err(format!("unsupported transport scheme: {}", scheme))
            }
        }
    }

    /// Send a request and return the response.
    async fn send(&self, request: IoRequest) -> Result<hppr_client::HpprResponse, String> {
        let conn = self.get_conn().await?;
        conn.send(request).await.map_err(|e| e.to_string())
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
        _account: &str,
        _token: &str,
    ) -> Result<(String, Vec<u8>), String> {
        let p = self.request_packet(
            IoRequest::Get { urc: urc.to_string() },
        ).await?;
        Ok(extract_content(&p))
    }

    /// GET returning validated Packet.
    async fn get_packet(&self, urc: &str) -> Result<Packet, String> {
        self.request_packet(
            IoRequest::Get { urc: urc.to_string() },
        ).await
    }

    /// GET returning validated Packet (public wrapper, legacy signature).
    pub async fn get_packet_authenticated(
        &self,
        urc: &str,
        _account: &str,
        _token: &str,
    ) -> Result<Packet, String> {
        self.get_packet(urc).await
    }

    /// LIST.
    pub async fn list(&self, urc: &str) -> Result<Vec<String>, String> {
        let text = self.request_lines(
            IoRequest::List { urc: urc.to_string() },
        ).await?;
        Ok(Self::parse_lines(&text))
    }

    /// MEMBERS.
    pub async fn members(&self, urc: &str) -> Result<Vec<String>, String> {
        let text = self.request_lines(
            IoRequest::Members { args: urc.to_string() },
        ).await?;
        Ok(Self::parse_lines(&text))
    }

    /// ADD.
    pub async fn add(&self, add_args: &[u8]) -> Result<String, String> {
        self.request_lines(
            IoRequest::Add { headers: add_args.to_vec(), data: None },
        ).await
    }

    // ========================================================================
    // Route Packet Methods
    // ========================================================================

    /// Get route info for a group/app from localhost.
    ///
    /// Route packets use the coordinate scheme:
    /// `//repo/admin/route/<group>/<app>/|/seal/<repo-vkey>`.
    pub async fn get_route(
        &self,
        group: &str,
        app: &str,
        repo_vkey: &str,
        _account: &str,
        _token: &str,
    ) -> Result<RouteInfo, String> {
        let urc = format!("//repo/admin/route/{}/{}/|/seal/{}", group, app, repo_vkey);
        let packet = self.get_packet(&urc).await?;

        // Verify it's a Seal packet
        let packet_type = hppr_packet::Packet::parse(packet.as_bytes().to_vec().into_boxed_slice())
            .map(|p| p.packet_type())
            .unwrap_or(PacketType::Null);
        if packet_type != PacketType::Seal {
            return Err(format!(
                "Route packet is not sealed (got type: {:?})",
                packet_type
            ));
        }

        // Extract and parse Upstream header (single via target)
        let upstream = match packet.header("Upstream") {
            Some(v) => Some(hppr_client::env_target::parse_via(v).map_err(|e| {
                format!("invalid Upstream header '{}': {}", v, e)
            })?),
            None => None,
        };

        if upstream.is_none() {
            return Err("Route packet has no Upstream header".to_string());
        }

        // Extract Upstream-Verification-Key header (repo's key from HELLO)
        let upstream_verification_key = packet
            .header("Upstream-Verification-Key")
            .map(|s| s.trim().to_string())
            .filter(|v| !v.is_empty());

        Ok(RouteInfo {
            upstream,
            upstream_verification_key,
        })
    }

    /// Get route key packet for a group from home admin namespace.
    pub async fn get_route_key(
        &self,
        group: &str,
        repo_vkey: &str,
        _account: &str,
        _token: &str,
    ) -> Result<RouteKeyInfo, String> {
        let urc = format!("//repo/admin/route-keys/{}/|/seal/{}", group, repo_vkey);
        let packet = self.get_packet(&urc).await?;

        let signing_key = packet
            .header("Secret-Key")
            .ok_or("Route key packet missing Secret-Key header")?
            .to_string();
        let _verification_key = packet
            .header("Verification-Key")
            .ok_or("Route key packet missing Verification-Key header")?
            .to_string();

        Ok(RouteKeyInfo { signing_key })
    }

    /// Ensure route key exists for a group. Creates one when missing.
    pub async fn ensure_route_key(
        &self,
        group: &str,
        repo_vkey: &str,
        account: &str,
        token: &str,
    ) -> Result<RouteKeyInfo, String> {
        if let Ok(existing) = self.get_route_key(group, repo_vkey, account, token).await {
            return Ok(existing);
        }

        let (signing_key, verification_key) = generate_keypair();
        let add_args = format!(
            "Seal-By: oldest\n\
             Group: repo\n\
             App: admin\n\
             Location: route-keys/{}\n\
             Secret-Key: {}\n\
             Verification-Key: {}\n",
            group, signing_key, verification_key
        );
        self.add(add_args.as_bytes()).await?;

        self.get_route_key(group, repo_vkey, account, token).await
    }

    /// Get admin identity (verification key) from host.
    ///
    /// Used for route lookups (coordinate scheme requires admin key).
    pub async fn get_admin_identity(&self, _account: &str, _token: &str) -> Result<String, String> {
        let urc = "//repo/admin/identity/|";
        let packet = self.get_packet(urc).await?;

        packet
            .header("Seal-By")
            .map(|s| s.to_string())
            .ok_or_else(|| "Host identity missing Seal-By header".to_string())
    }

    // ========================================================================
    // Site-Trust Methods (MEMBERS-compatible)
    // ========================================================================

    /// Get trusted verification keys from site-trust packet.
    ///
    /// Resolves //<g>/<a>/site-trust/|/seal/<admin-key> via MEMBERS.
    /// Returns empty vec if no site-trust exists.
    pub async fn get_site_trust_keys(
        &self,
        group: &str,
        app: &str,
        _account: &str,
        _token: &str,
    ) -> Vec<String> {
        let admin_key = match self.get_admin_identity("", "").await {
            Ok(key) => key,
            Err(_) => return vec![],
        };
        let app_part = if app.is_empty() {
            String::new()
        } else {
            format!("/{}", app)
        };
        let urc = format!("//{}{}/site-trust/|/seal/{}", group, app_part, admin_key);
        if let Ok(members_result) = self.members(&urc).await {
            return members_result
                .iter()
                .filter_map(|l| l.split_whitespace().next().map(String::from))
                .collect();
        }
        vec![]
    }

    /// Check if a key is trusted for a group/app via site-trust.
    pub async fn is_trusted(
        &self,
        group: &str,
        app: &str,
        trust_key: &str,
        _account: &str,
        _token: &str,
    ) -> bool {
        self.get_site_trust_keys(group, app, "", "")
            .await
            .iter()
            .any(|k| k == trust_key)
    }

    // ========================================================================
    // Site Ring1 Account Methods (Keypair-based)
    // ========================================================================

    /// Create site ring1 account for a group/app with keypair authentication.
    ///
    /// Generates a signing keypair and stores the site in the Ring1 Member list.
    pub async fn create_site_ring1(
        &self,
        group: &str,
        app: &str,
    ) -> Result<SiteCredential, String> {
        let ring1_name = site_ring1_name(group, app);
        let (signing_key, verification_key) = generate_keypair();

        // 1. Create Ring1 setup with Member header (no token)
        // Note: ACL-Rule headers must be in canonical order (040-ACCESS-CONTROL.md)
        let mut rules = vec![
            ("rdl", format!("//{}/{}/", group, app)),
            ("rwl", format!("//{}/{}/user/", group, app)),
            ("r..", "//repo/admin/route-keys/".to_string()),
            ("rwl", format!("//repo/admin/ring1/{}/", ring1_name)),
        ];
        rules.sort_by_key(|(_, path)| acl_coord_sort_key(path));
        let rules_str = rules
            .iter()
            .map(|(ops, path)| format!("ACL-Rule: {} {}", ops, path))
            .collect::<Vec<_>>()
            .join("\n");
        let setup_add_args = format!(
            "Seal-By: oldest\n\
             Group: repo\n\
             App: admin\n\
             Location: ring1/{}/setup\n\
             Member: {}\n\
             Ring1-Name: {}\n\
             {}\n",
            ring1_name, verification_key, ring1_name, rules_str
        );

        self.add(setup_add_args.as_bytes()).await?;

        // 2. Store signing key in Ring1 Keys (self-signed seal)
        let keys_add_args = format!(
            "Seal-By: {} {}\n\
             Group: repo\n\
             App: admin\n\
             Location: ring1/{}/keys\n\
             Secret-Key: {}\n",
            verification_key, signing_key, ring1_name, signing_key
        );

        self.add(keys_add_args.as_bytes()).await?;

        Ok(SiteCredential::new(
            ring1_name,
            signing_key,
            verification_key,
        ))
    }

    /// Load existing site ring1 credential from Ring1 Keys.
    pub async fn get_site_ring1_credential(
        &self,
        group: &str,
        app: &str,
    ) -> Result<SiteCredential, String> {
        let primary = site_ring1_name(group, app);
        let legacy = format!("HAVI-site:{}#{}", group, app);

        for ring1_name in [primary.clone(), legacy.clone()] {
            let keys_path = format!("//repo/admin/ring1/{}/keys/", ring1_name);
            let keys = match self.list(&keys_path).await {
                Ok(v) if !v.is_empty() => v,
                _ => continue,
            };

            let key_entry = keys.first().ok_or("No keys found for site Ring1")?;
            let verification_key = key_entry
                .trim_start_matches("|/seal/")
                .trim_end_matches('/');

            let key_urc = format!("{}|/seal/{}", keys_path, verification_key);
            let key_packet = self.get_packet(&key_urc).await?;

            let signing_key = key_packet
                .header("Secret-Key")
                .ok_or("Key packet missing Secret-Key header")?
                .to_string();

            return Ok(SiteCredential::new(
                primary.clone(),
                signing_key,
                verification_key.to_string(),
            ));
        }

        Err("No keys found for site Ring1".to_string())
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
    fn test_site_ring1_name_short() {
        // Test with short group/app names
        let name = site_ring1_name("chess", "game");
        assert_eq!(name, "site:chess#game");
    }

    #[test]
    fn test_site_ring1_name_long() {
        // Test with long group/app names (no truncation needed)
        let name = site_ring1_name("verylonggroupname", "verylongappname");
        assert_eq!(name, "site:verylonggroupname#verylongappname");
    }

    #[test]
    fn test_site_ring1_name_single_char() {
        // Test with minimal names (like spec example //u/web)
        let name = site_ring1_name("u", "web");
        assert_eq!(name, "site:u#web");
    }

    #[test]
    fn test_site_ring1_name_prefix() {
        let name = site_ring1_name("any", "app");
        assert!(name.starts_with("site:"));
    }

    #[test]
    fn test_site_ring1_name_deterministic() {
        // Same input should always produce same output
        let name1 = site_ring1_name("chess", "game");
        let name2 = site_ring1_name("chess", "game");
        assert_eq!(name1, name2);
    }

    #[test]
    fn test_site_ring1_name_unique() {
        // Different inputs should produce different outputs
        let name1 = site_ring1_name("chess", "game");
        let name2 = site_ring1_name("chess", "games");
        let name3 = site_ring1_name("ches", "game");
        assert_ne!(name1, name2);
        assert_ne!(name1, name3);
        assert_ne!(name2, name3);
    }

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
