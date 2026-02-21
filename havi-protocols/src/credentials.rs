/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Centralized credential store for HPPR protocol handlers.
//!
//! Provides type-safe credential management with:
//! - Admin credentials keyed by repo verification key at `<credentials_dir>/<key>`
//! - Per-group/app sandbox credential caching
//! - Thread-safe access via Arc/RwLock with safe error handling
//!
//! Credentials directory resolution is handled by `config::credentials_dir()`.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};

use crate::client::HpprdClientAsync;
use crate::config;

/// Default ring0 account name per spec 090 bootstrap.
pub const DEFAULT_RING0_NAME: &str = "ring0";

/// Default root token per spec 090 bootstrap.
pub const DEFAULT_ROOT_TOKEN: &str = "init";

/// Admin credential with token authentication (for Ring0/admin access).
#[derive(Debug, Clone)]
pub struct Credential {
    /// Ring1 account name.
    pub ring1_name: String,
    /// Secret token (private - never exposed to DOM).
    token: String,
}

impl Credential {
    /// Create a new credential.
    pub fn new(ring1_name: String, token: String) -> Self {
        Self {
            ring1_name,
            token,
        }
    }

    /// Get the token for internal use only.
    pub fn token(&self) -> &str {
        &self.token
    }
}

/// Site credential with signing keypair for seal authentication.
///
/// Sites are authenticated via Ring1 Member list, not tokens.
/// The signing key is used to create sealed requests.
#[derive(Debug, Clone)]
pub struct SiteCredential {
    /// Ring1 account name (e.g., "site:chess#games").
    pub ring1_name: String,
    /// Signing key (private, &.xxx.H3 format).
    signing_key: String,
    /// Verification key (public, V.xxx.H3 format).
    pub verification_key: String,
}

impl SiteCredential {
    /// Create a new site credential.
    pub fn new(ring1_name: String, signing_key: String, verification_key: String) -> Self {
        Self {
            ring1_name,
            signing_key,
            verification_key,
        }
    }

    /// Get the signing key for internal use only.
    pub fn signing_key(&self) -> &str {
        &self.signing_key
    }
}

/// Route credential with per-group keypair for Ring2 remote authentication.
#[derive(Debug, Clone)]
pub struct RouteCredential {
    signing_key: String,
}

impl RouteCredential {
    pub fn new(signing_key: String) -> Self {
        Self { signing_key }
    }

    pub fn signing_key(&self) -> &str {
        &self.signing_key
    }
}

/// Central credential store with persistence.
pub struct CredentialStore {
    /// Admin credential (persisted to disk, keyed by repo verification key).
    admin: RwLock<Option<Credential>>,
    /// Cached site credentials with keypairs by (group, app).
    site_credentials: RwLock<HashMap<(String, String), SiteCredential>>,
    /// Cached route credentials with keypairs by group.
    route_credentials: RwLock<HashMap<String, RouteCredential>>,
    /// Directory for credential files.
    /// See `config::credentials_dir()` for resolution order.
    credentials_dir: PathBuf,
}

impl CredentialStore {
    /// Create a new credential store with the given credentials directory.
    ///
    /// Does NOT load credentials automatically - call `load_admin_for_key()` after
    /// connecting to hpprd and getting the repo verification key via HELLO.
    pub fn new(credentials_dir: PathBuf) -> Self {
        Self {
            admin: RwLock::new(None),
            site_credentials: RwLock::new(HashMap::new()),
            route_credentials: RwLock::new(HashMap::new()),
            credentials_dir,
        }
    }

    /// Create a new credential store using the default credentials directory.
    ///
    /// Uses `config::credentials_dir()` for path resolution.
    pub fn new_default() -> Self {
        Self::new(config::credentials_dir())
    }

    /// Get the file path for a repo verification key.
    ///
    /// If key is "0" (no key configured), uses "_default" as filename.
    fn credential_path_for_key(&self, key: &str) -> PathBuf {
        let filename = if key == "0" { "_default" } else { key };
        self.credentials_dir.join(filename)
    }

    /// Load admin credential for a specific repo key.
    ///
    /// Called after HELLO returns the repo verification key.
    pub fn load_admin_for_key(&self, key: &str) -> Result<(), String> {
        let path = self.credential_path_for_key(key);
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        let mut lines = content.lines();
        let account = lines.next().ok_or("Missing account line")?.to_string();
        let token = lines.next().ok_or("Missing token line")?.to_string();

        if let Ok(mut admin) = self.admin.write() {
            *admin = Some(Credential::new(account, token));
            log::info!("Loaded admin credential for key {} from {}", key, path.display());
            Ok(())
        } else {
            Err("Failed to acquire write lock".to_string())
        }
    }

    /// Persist admin credential for a specific repo key.
    pub fn persist_admin_for_key(&self, key: &str) -> Result<(), String> {
        let admin = self.admin.read()
            .map_err(|_| "Failed to acquire read lock")?;

        let cred = admin.as_ref().ok_or("No admin credential to persist")?;

        fs::create_dir_all(&self.credentials_dir)
            .map_err(|e| format!("Failed to create credentials dir: {}", e))?;

        let path = self.credential_path_for_key(key);
        let content = format!("{}\n{}\n", cred.ring1_name, cred.token);

        fs::write(&path, &content)
            .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;

        log::info!("Persisted admin credential for key {} to {}", key, path.display());
        Ok(())
    }

    /// Get the current admin credential.
    pub fn get_admin(&self) -> Option<Credential> {
        self.admin.read().ok()?.clone()
    }

    /// Set the admin credential.
    #[cfg(test)]
    pub fn set_admin(&self, credential: Credential) -> Result<(), String> {
        if let Ok(mut admin) = self.admin.write() {
            *admin = Some(credential);
            Ok(())
        } else {
            Err("Failed to acquire write lock".to_string())
        }
    }

    /// Bootstrap admin credential if none exists.
    pub fn bootstrap_admin(&self) {
        if self.get_admin().is_none() {
            let cred = Credential::new(
                DEFAULT_RING0_NAME.to_string(),
                DEFAULT_ROOT_TOKEN.to_string(),
            );
            if let Ok(mut admin) = self.admin.write() {
                *admin = Some(cred);
                log::info!("Bootstrapped admin credential with default ring0/init");
            }
        }
    }

    /// Get or create a site credential for a group/app (async version).
    ///
    /// Site credentials use keypair-based seal authentication via Ring1 Member list.
    pub async fn get_or_create_site_credential_async(
        &self,
        group: &str,
        app: &str,
        client: &HpprdClientAsync,
    ) -> Result<SiteCredential, String> {
        let key = (group.to_string(), app.to_string());

        if let Ok(cache) = self.site_credentials.read() {
            if let Some(cred) = cache.get(&key) {
                return Ok(cred.clone());
            }
        }

        let cred = match client.get_site_ring1_credential(group, app).await {
            Ok(cred) => cred,
            Err(_) => client.create_site_ring1(group, app).await?,
        };

        if let Ok(mut cache) = self.site_credentials.write() {
            cache.insert(key, cred.clone());
        }

        log::info!(
            "Site credential ready for {}/{}: {}",
            group,
            app,
            cred.verification_key
        );
        Ok(cred)
    }

    /// Get or create a route credential for a group (async version).
    pub async fn get_or_create_route_credential_async(
        &self,
        group: &str,
        client: &HpprdClientAsync,
    ) -> Result<RouteCredential, String> {
        if let Ok(cache) = self.route_credentials.read() {
            if let Some(cred) = cache.get(group) {
                return Ok(cred.clone());
            }
        }

        let admin = self
            .get_admin()
            .ok_or("No admin credential available for route key lookup")?;
        let repo_vkey = client
            .get_admin_identity(&admin.ring1_name, admin.token())
            .await?;
        let route_key = client
            .ensure_route_key(group, &repo_vkey, &admin.ring1_name, admin.token())
            .await?;

        let cred = RouteCredential::new(route_key.signing_key);
        if let Ok(mut cache) = self.route_credentials.write() {
            cache.insert(group.to_string(), cred.clone());
        }

        Ok(cred)
    }

}

/// Handle for sharing credential store across components.
pub type CredentialStoreHandle = Arc<CredentialStore>;

/// Global credential store singleton.
///
/// Uses OnceLock to ensure a single CredentialStore instance is shared across
/// all protocol handlers and embedder components. This eliminates duplicate
/// sandbox account creation and enables credential caching.
static GLOBAL_CREDENTIAL_STORE: OnceLock<CredentialStoreHandle> = OnceLock::new();

/// Get the global credential store singleton.
///
/// Creates the store on first access using `CredentialStore::new_default()`.
/// All subsequent calls return the same instance.
pub fn global_credential_store() -> CredentialStoreHandle {
    GLOBAL_CREDENTIAL_STORE
        .get_or_init(|| Arc::new(CredentialStore::new_default()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credential_new() {
        let cred = Credential::new("alice".to_string(), "secret123".to_string());
        assert_eq!(cred.ring1_name, "alice");
        assert_eq!(cred.token(), "secret123");
    }

    #[test]
    fn test_credential_store_bootstrap() {
        let temp_dir = std::env::temp_dir().join("hppr_test_bootstrap");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let store = CredentialStore::new(temp_dir.clone());
        assert!(store.get_admin().is_none());

        store.bootstrap_admin();
        let admin = store.get_admin().unwrap();
        assert_eq!(admin.ring1_name, DEFAULT_RING0_NAME);
        assert_eq!(admin.token(), DEFAULT_ROOT_TOKEN);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_credential_store_persistence_by_key() {
        let temp_dir = std::env::temp_dir().join("hppr_test_persist_key");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let server_key = "V.testkey123.H3";

        // Store credential for a specific repo key
        {
            let store = CredentialStore::new(temp_dir.clone());
            let cred = Credential::new("myaccount".to_string(), "mytoken".to_string());
            store.set_admin(cred).unwrap();
            store.persist_admin_for_key(server_key).unwrap();
        }

        // Load credential for the same repo key
        {
            let store = CredentialStore::new(temp_dir.clone());
            store.load_admin_for_key(server_key).unwrap();
            let admin = store.get_admin().unwrap();
            assert_eq!(admin.ring1_name, "myaccount");
            assert_eq!(admin.token(), "mytoken");
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_credential_store_separate_servers() {
        let temp_dir = std::env::temp_dir().join("hppr_test_multi_server");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let key1 = "V.server1.H3";
        let key2 = "V.server2.H3";

        // Store credentials for server 1
        {
            let store = CredentialStore::new(temp_dir.clone());
            let cred = Credential::new("alice".to_string(), "token1".to_string());
            store.set_admin(cred).unwrap();
            store.persist_admin_for_key(key1).unwrap();
        }

        // Store credentials for server 2
        {
            let store = CredentialStore::new(temp_dir.clone());
            let cred = Credential::new("bob".to_string(), "token2".to_string());
            store.set_admin(cred).unwrap();
            store.persist_admin_for_key(key2).unwrap();
        }

        // Verify server 1 credentials are separate
        {
            let store = CredentialStore::new(temp_dir.clone());
            store.load_admin_for_key(key1).unwrap();
            let admin = store.get_admin().unwrap();
            assert_eq!(admin.ring1_name, "alice");
        }

        // Verify server 2 credentials are separate
        {
            let store = CredentialStore::new(temp_dir.clone());
            store.load_admin_for_key(key2).unwrap();
            let admin = store.get_admin().unwrap();
            assert_eq!(admin.ring1_name, "bob");
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_credential_path_for_zero_key() {
        let temp_dir = std::env::temp_dir().join("hppr_test_zero_key");
        let store = CredentialStore::new(temp_dir.clone());

        // Key "0" should map to "_default" filename
        let path = store.credential_path_for_key("0");
        assert!(path.ends_with("_default"));

        // Normal keys should use key as filename
        let path = store.credential_path_for_key("V.abc.H3");
        assert!(path.ends_with("V.abc.H3"));
    }

    #[test]
    fn test_site_credential_new() {
        let cred = SiteCredential::new(
            "site:chess#games".to_string(),
            "&.signingkey.H3".to_string(),
            "V.verifykey.H3".to_string(),
        );
        assert_eq!(cred.ring1_name, "site:chess#games");
        assert_eq!(cred.signing_key(), "&.signingkey.H3");
        assert_eq!(cred.verification_key, "V.verifykey.H3");
    }
}
