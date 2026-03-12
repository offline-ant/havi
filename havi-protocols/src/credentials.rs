/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Centralized credential store for HPPR protocol handlers.
//!
//! Persistence is SQLite-backed in `<config_dir>/havi.sqlite`.

use std::collections::HashMap;
#[cfg(test)]
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

use crate::client::HpprdClientAsync;
use crate::state_db::{ShadowKeyEntry, StateDbHandle, global_state_db};

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
        Self { ring1_name, token }
    }

    /// Get the token for internal use only.
    pub fn token(&self) -> &str {
        &self.token
    }
}

/// Site credential with signing keypair for seal authentication.
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

#[derive(Debug, Clone)]
pub struct ShadowCredential {
    pub name: String,
    signing_key: String,
    pub verification_key: String,
}

impl ShadowCredential {
    pub fn new(name: String, signing_key: String, verification_key: String) -> Self {
        Self {
            name,
            signing_key,
            verification_key,
        }
    }

    pub fn signing_key(&self) -> &str {
        &self.signing_key
    }
}

fn shadow_signer_name(group: &str, app: &str) -> String {
    format!("shadow:{}#{}", group, app)
}

fn shadow_credential_from_row(row: ShadowKeyEntry) -> ShadowCredential {
    ShadowCredential::new(
        shadow_signer_name(&row.group, &row.app),
        row.signing_key,
        row.verification_key,
    )
}

/// Central credential store with SQLite persistence.
pub struct CredentialStore {
    /// Admin credential currently active in process memory.
    admin: RwLock<Option<Credential>>,
    /// Cached site credentials with keypairs by (group, app).
    site_credentials: RwLock<HashMap<(String, String), SiteCredential>>,
    /// Cached route credentials with keypairs by group.
    route_credentials: RwLock<HashMap<String, RouteCredential>>,
    /// Cached persistent shadow credentials with keypairs by (group, app).
    shadow_credentials: RwLock<HashMap<(String, String), ShadowCredential>>,
    /// Shared state database.
    db: StateDbHandle,
}

impl CredentialStore {
    /// Create a credential store using the provided state DB handle.
    pub fn new(db: StateDbHandle) -> Self {
        Self {
            admin: RwLock::new(None),
            site_credentials: RwLock::new(HashMap::new()),
            route_credentials: RwLock::new(HashMap::new()),
            shadow_credentials: RwLock::new(HashMap::new()),
            db,
        }
    }

    /// Create a new credential store using the global default state DB.
    pub fn new_default() -> Self {
        Self::new(global_state_db())
    }

    /// Create a store with a dedicated test DB path.
    #[cfg(test)]
    pub fn new_test(path: &Path) -> Self {
        Self::new(crate::state_db::open_test_db(path))
    }

    /// Load admin credential for a specific repo key.
    pub fn load_admin_for_key(&self, key: &str) -> Result<(), String> {
        let Some((account, token)) = self.db.get_credential(key)? else {
            return Err(format!("no credential stored for key {}", key));
        };

        if let Ok(mut admin) = self.admin.write() {
            *admin = Some(Credential::new(account, token));
            Ok(())
        } else {
            Err("Failed to acquire write lock".to_string())
        }
    }

    /// Persist admin credential for a specific repo key.
    pub fn persist_admin_for_key(&self, key: &str) -> Result<(), String> {
        let admin = self
            .admin
            .read()
            .map_err(|_| "Failed to acquire read lock")?;

        let cred = admin.as_ref().ok_or("No admin credential to persist")?;
        self.db.set_credential(key, &cred.ring1_name, cred.token())
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

        self.get_admin()
            .ok_or("No admin credential available for route key lookup")?;
        let repo_vkey = client
            .get_admin_identity()
            .await?;
        let route_key = client
            .ensure_route_key(group, &repo_vkey)
            .await?;

        let cred = RouteCredential::new(route_key.signing_key);
        if let Ok(mut cache) = self.route_credentials.write() {
            cache.insert(group.to_string(), cred.clone());
        }

        Ok(cred)
    }

    pub fn get_or_create_shadow_credential(
        &self,
        group: &str,
        app: &str,
    ) -> Result<ShadowCredential, String> {
        let key = (group.to_string(), app.to_string());

        if let Ok(cache) = self.shadow_credentials.read() {
            if let Some(cred) = cache.get(&key) {
                return Ok(cred.clone());
            }
        }

        let cred = if let Some(row) = self.db.get_shadow_key(group, app)? {
            shadow_credential_from_row(row)
        } else {
            let (signing_key, verification_key) = hppr_packet::crypto::generate_signing_verifying_pair();
            let cred = ShadowCredential::new(
                shadow_signer_name(group, app),
                signing_key,
                verification_key,
            );
            self.db.set_shadow_key(
                group,
                app,
                cred.signing_key(),
                &cred.verification_key,
            )?;
            cred
        };

        if let Ok(mut cache) = self.shadow_credentials.write() {
            cache.insert(key, cred.clone());
        }

        Ok(cred)
    }
}

/// Handle for sharing credential store across components.
pub type CredentialStoreHandle = Arc<CredentialStore>;

/// Global credential store singleton.
static GLOBAL_CREDENTIAL_STORE: OnceLock<CredentialStoreHandle> = OnceLock::new();

/// Get the global credential store singleton.
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
        let db_path = std::env::temp_dir().join("havi_test_bootstrap.sqlite");
        let _ = std::fs::remove_file(&db_path);

        let store = CredentialStore::new_test(&db_path);
        assert!(store.get_admin().is_none());

        store.bootstrap_admin();
        let admin = store.get_admin().unwrap();
        assert_eq!(admin.ring1_name, DEFAULT_RING0_NAME);
        assert_eq!(admin.token(), DEFAULT_ROOT_TOKEN);

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_credential_store_persistence_by_key() {
        let db_path = std::env::temp_dir().join("havi_test_persist_key.sqlite");
        let _ = std::fs::remove_file(&db_path);

        let server_key = "V.testkey123.H3";

        {
            let store = CredentialStore::new_test(&db_path);
            let cred = Credential::new("myaccount".to_string(), "mytoken".to_string());
            store.set_admin(cred).unwrap();
            store.persist_admin_for_key(server_key).unwrap();
        }

        {
            let store = CredentialStore::new_test(&db_path);
            store.load_admin_for_key(server_key).unwrap();
            let admin = store.get_admin().unwrap();
            assert_eq!(admin.ring1_name, "myaccount");
            assert_eq!(admin.token(), "mytoken");
        }

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_credential_store_separate_servers() {
        let db_path = std::env::temp_dir().join("havi_test_multi_server.sqlite");
        let _ = std::fs::remove_file(&db_path);

        let key1 = "V.server1.H3";
        let key2 = "V.server2.H3";

        {
            let store = CredentialStore::new_test(&db_path);
            let cred = Credential::new("alice".to_string(), "token1".to_string());
            store.set_admin(cred).unwrap();
            store.persist_admin_for_key(key1).unwrap();
        }

        {
            let store = CredentialStore::new_test(&db_path);
            let cred = Credential::new("bob".to_string(), "token2".to_string());
            store.set_admin(cred).unwrap();
            store.persist_admin_for_key(key2).unwrap();
        }

        {
            let store = CredentialStore::new_test(&db_path);
            store.load_admin_for_key(key1).unwrap();
            let admin = store.get_admin().unwrap();
            assert_eq!(admin.ring1_name, "alice");
        }

        {
            let store = CredentialStore::new_test(&db_path);
            store.load_admin_for_key(key2).unwrap();
            let admin = store.get_admin().unwrap();
            assert_eq!(admin.ring1_name, "bob");
        }

        let _ = std::fs::remove_file(db_path);
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

    #[test]
    fn test_shadow_credential_persistent_per_origin() {
        let db_path = std::env::temp_dir().join("havi_test_shadow_key.sqlite");
        let _ = std::fs::remove_file(&db_path);

        let store = CredentialStore::new_test(&db_path);
        let first = store.get_or_create_shadow_credential("dev", "hppr.forge").unwrap();
        let second = store.get_or_create_shadow_credential("dev", "hppr.forge").unwrap();
        assert_eq!(first.name, "shadow:dev#hppr.forge");
        assert_eq!(first.signing_key(), second.signing_key());
        assert_eq!(first.verification_key, second.verification_key);

        let _ = std::fs::remove_file(db_path);
    }
}
