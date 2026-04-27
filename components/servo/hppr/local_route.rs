/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Browser-facing handle for browser-local route/trust state reads.
//!
//! `BrowserRouteHandle` wraps an hpprd client and credential store to:
//! - centralize the route/trust read path against the current compatibility repo
//! - cache the admin identity so it is fetched at most once per handle lifetime
//!
//! The backing store currently remains the hpprd compatibility daemon via
//! `HpprdClientAsync`.

use std::sync::Arc;

use hppr_client::ViaSpec;
use tokio::sync::OnceCell;

use super::client::{HpprdClientAsync, LocalRouteAppInfo, LocalRouteGroupInfo, RouteAuthInfo};
use super::credentials::CredentialStoreHandle;

/// Browser-facing handle for browser-local route/trust state reads.
///
/// Create one per top-level resolution call. Admin identity is fetched at most
/// once and cached for the lifetime of this handle.
pub struct BrowserRouteHandle {
    client: Arc<HpprdClientAsync>,
    credential_store: CredentialStoreHandle,
    cached_identity: OnceCell<Option<String>>,
}

impl BrowserRouteHandle {
    pub fn new(client: Arc<HpprdClientAsync>, credential_store: CredentialStoreHandle) -> Self {
        Self {
            client,
            credential_store,
            cached_identity: OnceCell::new(),
        }
    }

    pub fn target(&self) -> ViaSpec {
        self.client.target()
    }

    /// Repo verification key used to seal local route records.
    ///
    /// Returns `None` if no admin credentials are present or if the
    /// compatibility repo is unreachable. Result is cached for the lifetime of
    /// this handle.
    pub async fn admin_identity(&self) -> Option<String> {
        self.cached_identity
            .get_or_init(|| async {
                if self.credential_store.get_admin().is_none() {
                    return None;
                }
                self.client.get_admin_identity().await.ok()
            })
            .await
            .clone()
    }

    pub async fn local_route_group(
        &self,
        group: &str,
        repo_vkey: &str,
    ) -> Result<LocalRouteGroupInfo, String> {
        self.client.get_local_route_group(group, repo_vkey).await
    }

    pub async fn local_route_app(
        &self,
        group: &str,
        app: &str,
        repo_vkey: &str,
    ) -> Result<LocalRouteAppInfo, String> {
        self.client.get_local_route_app(group, app, repo_vkey).await
    }

    pub async fn route_auth(
        &self,
        group: &str,
        app: Option<&str>,
        repo_vkey: &str,
    ) -> Result<RouteAuthInfo, String> {
        self.client.get_route_auth(group, app, repo_vkey).await
    }
}
