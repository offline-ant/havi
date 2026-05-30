/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Browser-facing handle for browser-local route/trust state reads.
//!
//! This stays on the browser-owned packet-store path when `HAVI_HOME` is unset
//! and uses an explicit remote hpprd client only when the browser is running
//! against a user-provided remote home repo.

use std::sync::Arc;

use hppr_client::ViaSpec;
use tokio::sync::OnceCell;

use super::client::{HpprdClientAsync, LocalRouteApiInfo, LocalRouteGroupInfo, RouteAuthInfo};
use super::credentials::CredentialStoreHandle;
use super::local_runtime::{default_repo_backed_runtime_is_local, global_local_runtime};
use super::repo_target;

enum BrowserRouteBackend {
    Local,
    Remote {
        client: Arc<HpprdClientAsync>,
        credential_store: CredentialStoreHandle,
    },
}

/// Browser-facing handle for browser-local route/trust state reads.
///
/// Create one per top-level resolution call. Admin identity is fetched at most
/// once and cached for the lifetime of this handle.
pub struct BrowserRouteHandle {
    backend: BrowserRouteBackend,
    cached_identity: OnceCell<Option<String>>,
}

impl BrowserRouteHandle {
    pub fn new(client: Arc<HpprdClientAsync>, credential_store: CredentialStoreHandle) -> Self {
        let backend = if default_repo_backed_runtime_is_local() {
            BrowserRouteBackend::Local
        } else {
            BrowserRouteBackend::Remote {
                client,
                credential_store,
            }
        };
        Self {
            backend,
            cached_identity: OnceCell::new(),
        }
    }

    pub fn target_display(&self) -> String {
        match &self.backend {
            BrowserRouteBackend::Local => "repo".to_string(),
            BrowserRouteBackend::Remote { client, .. } => client.target().to_string(),
        }
    }

    pub fn fallback_target(&self) -> ViaSpec {
        match &self.backend {
            BrowserRouteBackend::Local => repo_target::get(),
            BrowserRouteBackend::Remote { client, .. } => client.target(),
        }
    }

    /// Repo verification key used to seal local route records.
    ///
    /// Returns `None` if no admin credentials are present or if the remote home
    /// repo is unreachable. Result is cached for the lifetime of this handle.
    pub async fn admin_identity(&self) -> Option<String> {
        self.cached_identity
            .get_or_init(|| async {
                match &self.backend {
                    BrowserRouteBackend::Local => {
                        Some(global_local_runtime().verifying_key().to_string())
                    }
                    BrowserRouteBackend::Remote {
                        client,
                        credential_store,
                    } => {
                        if credential_store.get_admin().is_none() {
                            return None;
                        }
                        client.get_admin_identity().await.ok()
                    }
                }
            })
            .await
            .clone()
    }

    pub async fn local_route_group(
        &self,
        group: &str,
        repo_vkey: &str,
    ) -> Result<LocalRouteGroupInfo, String> {
        match &self.backend {
            BrowserRouteBackend::Local => {
                global_local_runtime().get_local_route_group(group, repo_vkey)
            }
            BrowserRouteBackend::Remote { client, .. } => {
                client.get_local_route_group(group, repo_vkey).await
            }
        }
    }

    pub async fn local_route_api(
        &self,
        group: &str,
        api: &str,
        repo_vkey: &str,
    ) -> Result<LocalRouteApiInfo, String> {
        match &self.backend {
            BrowserRouteBackend::Local => {
                global_local_runtime().get_local_route_api(group, api, repo_vkey)
            }
            BrowserRouteBackend::Remote { client, .. } => {
                client.get_local_route_api(group, api, repo_vkey).await
            }
        }
    }

    pub async fn route_auth(
        &self,
        group: &str,
        api: Option<&str>,
        repo_vkey: &str,
    ) -> Result<RouteAuthInfo, String> {
        match &self.backend {
            BrowserRouteBackend::Local => global_local_runtime().get_route_auth(group, api, repo_vkey),
            BrowserRouteBackend::Remote { client, .. } => {
                client.get_route_auth(group, api, repo_vkey).await
            }
        }
    }
}
