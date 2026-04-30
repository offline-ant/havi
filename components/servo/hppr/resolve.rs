/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared HPPR source resolution.
//!
//! This module owns browser-level consequences of HPPR route resolution:
//! - route endpoint selection
//! - local route auth attachment
//! - content-pointer resolution
//! - document/media packet resolution
//! - source-based byte reads
//!
//! Route record structure and effective/canonical resolution semantics are
//! defined by the HPPR route scheme, not by HAVI-specific packet rules.

use std::sync::Arc;

use hppr_client::{Packet, Signer, parse_via};
use hppr_packet::chunk::{ChunkKind, ChunkManifest, is_chunk_manifest, parse_chunk_manifest};
use hppr_packet::urc::UrcMethod;
use net_traits::{HpprDocumentSource, HpprDocumentSourceSnapshot};

use super::client::{ContentPointerInfo, HpprdClientAsync};
use super::credentials::CredentialStoreHandle;
use super::local_route::BrowserRouteHandle;
use super::local_runtime::{default_repo_backed_runtime_is_local, global_local_runtime};
use super::state_db::global_state_db;
use super::url::HAVIAddress;
use super::util::{
    RouteEndpointSource, append_location, resolve_route_endpoint_with_trace, shadow_root,
};

#[derive(Clone, Debug)]
pub struct ResolvedSourceRef {
    pub endpoint: String,
    pub signer: Option<Signer>,
    pub content_authority: Option<String>,
    pub packet_hash: String,
    pub is_repo: bool,
}

#[derive(Clone, Debug)]
pub struct ResolvedDocument {
    pub packet: Packet,
    pub endpoint: String,
    pub signer: Option<Signer>,
    pub content_authority: Option<String>,
    pub is_repo: bool,
    pub hppr_source: HpprDocumentSource,
    pub source: ResolvedSourceRef,
    pub lookup_trace: embedder_traits::HpprLookupTrace,
}

#[derive(Clone, Debug)]
pub struct ResolvedMediaSource {
    pub packet: Packet,
    pub endpoint: String,
    pub signer: Option<Signer>,
    pub content_authority: Option<String>,
    pub is_repo: bool,
    pub hppr_source: HpprDocumentSource,
    pub source: ResolvedSourceRef,
    pub lookup_trace: embedder_traits::HpprLookupTrace,
}

#[derive(Clone, Debug)]
pub struct ResolvedListing {
    pub children: Vec<String>,
    pub endpoint: String,
    pub signer: Option<Signer>,
    pub content_authority: Option<String>,
    pub is_repo: bool,
    pub hppr_source: HpprDocumentSource,
    pub lookup_trace: embedder_traits::HpprLookupTrace,
}

enum ResolvedClient {
    Remote(Arc<HpprdClientAsync>),
    Local(super::local_runtime::BrowserLocalRuntimeHandle),
}

impl ResolvedClient {
    async fn get_packet(&self, urc: &str) -> Result<Packet, String> {
        match self {
            ResolvedClient::Remote(client) => client.get_packet_authenticated(urc).await,
            ResolvedClient::Local(runtime) => runtime.get_packet(urc),
        }
    }

    async fn list(&self, urc: &str) -> Result<Vec<String>, String> {
        match self {
            ResolvedClient::Remote(client) => client.list(urc).await,
            ResolvedClient::Local(runtime) => runtime.list_entries(urc),
        }
    }
}

struct ResolvedAccess {
    endpoint: String,
    signer: Option<Signer>,
    content_authority: Option<String>,
    is_repo: bool,
    client: ResolvedClient,
    urc: String,
}

#[derive(Clone, Debug)]
pub struct RouteResolvedDocumentSource {
    pub snapshot: HpprDocumentSourceSnapshot,
    pub source: RouteEndpointSource,
    pub lookup_trace: embedder_traits::HpprLookupTrace,
}

#[derive(Clone, Debug)]
pub struct HpprResolveError {
    pub message: String,
    pub lookup_trace: embedder_traits::HpprLookupTrace,
}

impl HpprResolveError {
    fn new(message: impl Into<String>, mut lookup_trace: embedder_traits::HpprLookupTrace) -> Self {
        let message = message.into();
        lookup_trace.set_terminal_error(message.clone());
        Self {
            message,
            lookup_trace,
        }
    }
}

impl std::fmt::Display for HpprResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}

impl std::error::Error for HpprResolveError {}

pub async fn route_configured_for_direct_endpoint(
    group: &str,
    app: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> bool {
    if group.is_empty() || app.is_empty() {
        return false;
    }
    let handle = BrowserRouteHandle::new(repo_client.clone(), credential_store.clone());
    let Some(repo_vkey) = handle.admin_identity().await else {
        return false;
    };
    handle.local_route_app(group, app, &repo_vkey).await.is_ok()
        || handle.local_route_group(group, &repo_vkey).await.is_ok()
}

pub async fn resolve_document(
    url: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> Result<ResolvedDocument, HpprResolveError> {
    resolve_document_with_snapshot(url, repo_client, credential_store, None).await
}

pub async fn resolve_document_with_snapshot(
    url: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
    snapshot: Option<&HpprDocumentSourceSnapshot>,
) -> Result<ResolvedDocument, HpprResolveError> {
    let address = HAVIAddress::parse(url).map_err(|e| {
        HpprResolveError::new(
            e.to_string(),
            embedder_traits::HpprLookupTrace::new(url.to_string(), "document"),
        )
    })?;
    if address.is_listing() {
        return Err(HpprResolveError::new(
            "listing URL cannot resolve to a document source",
            embedder_traits::HpprLookupTrace::new(url.to_string(), "document"),
        ));
    }

    let mut route_source =
        resolve_document_source(&address, repo_client, credential_store, snapshot).await?;
    let access = resolve_access(&address, repo_client, &route_source.snapshot, false).map_err(
        |error| HpprResolveError::new(error, route_source.lookup_trace.clone()),
    )?;
    route_source.lookup_trace.set_final_target(access.urc.clone());
    let packet = match access.client.get_packet(&access.urc).await {
        Ok(packet) => packet,
        Err(error) => {
            return Err(HpprResolveError::new(error, route_source.lookup_trace));
        },
    };
    let content_authority = access.content_authority.or_else(|| packet_content_authority(&packet));
    route_source.lookup_trace.push_step(
        "document-fetch",
        Some(access.urc.clone()),
        Some(access.endpoint.clone()),
        "hit",
        Some(format!("packet={}", packet.pkt_hash())),
    );
    let source = ResolvedSourceRef {
        endpoint: access.endpoint.clone(),
        signer: access.signer.clone(),
        content_authority: content_authority.clone(),
        packet_hash: packet.pkt_hash().to_string(),
        is_repo: access.is_repo,
    };

    Ok(ResolvedDocument {
        packet,
        endpoint: access.endpoint,
        signer: access.signer,
        content_authority,
        is_repo: access.is_repo,
        hppr_source: route_source.snapshot.source.clone(),
        source,
        lookup_trace: route_source.lookup_trace,
    })
}

pub async fn resolve_media(
    url: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> Result<ResolvedMediaSource, HpprResolveError> {
    resolve_media_with_snapshot(url, repo_client, credential_store, None).await
}

pub async fn resolve_media_with_snapshot(
    url: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
    snapshot: Option<&HpprDocumentSourceSnapshot>,
) -> Result<ResolvedMediaSource, HpprResolveError> {
    let address = HAVIAddress::parse(url).map_err(|e| {
        HpprResolveError::new(
            e.to_string(),
            embedder_traits::HpprLookupTrace::new(url.to_string(), "media"),
        )
    })?;
    if address.is_listing() {
        return Err(HpprResolveError::new(
            "listing URL cannot resolve to a media source",
            embedder_traits::HpprLookupTrace::new(url.to_string(), "media"),
        ));
    }

    let mut route_source =
        resolve_document_source(&address, repo_client, credential_store, snapshot).await?;
    let access = resolve_access(&address, repo_client, &route_source.snapshot, false).map_err(
        |error| HpprResolveError::new(error, route_source.lookup_trace.clone()),
    )?;
    route_source.lookup_trace.set_final_target(access.urc.clone());
    let packet = match access.client.get_packet(&access.urc).await {
        Ok(packet) => packet,
        Err(error) => {
            return Err(HpprResolveError::new(error, route_source.lookup_trace));
        },
    };
    let content_authority = access.content_authority.or_else(|| packet_content_authority(&packet));
    route_source.lookup_trace.push_step(
        "media-fetch",
        Some(access.urc.clone()),
        Some(access.endpoint.clone()),
        "hit",
        Some(format!("packet={}", packet.pkt_hash())),
    );
    let source = ResolvedSourceRef {
        endpoint: access.endpoint.clone(),
        signer: access.signer.clone(),
        content_authority: content_authority.clone(),
        packet_hash: packet.pkt_hash().to_string(),
        is_repo: access.is_repo,
    };

    Ok(ResolvedMediaSource {
        packet,
        endpoint: access.endpoint,
        signer: access.signer,
        content_authority,
        is_repo: access.is_repo,
        hppr_source: route_source.snapshot.source.clone(),
        source,
        lookup_trace: route_source.lookup_trace,
    })
}

pub async fn resolve_listing(
    url: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> Result<ResolvedListing, HpprResolveError> {
    resolve_listing_with_snapshot(url, repo_client, credential_store, None).await
}

pub async fn resolve_listing_with_snapshot(
    url: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
    snapshot: Option<&HpprDocumentSourceSnapshot>,
) -> Result<ResolvedListing, HpprResolveError> {
    let address = HAVIAddress::parse(url).map_err(|e| {
        HpprResolveError::new(
            e.to_string(),
            embedder_traits::HpprLookupTrace::new(url.to_string(), "listing"),
        )
    })?;
    let mut route_source =
        resolve_document_source(&address, repo_client, credential_store, snapshot).await?;
    let access = resolve_access(&address, repo_client, &route_source.snapshot, true).map_err(
        |error| HpprResolveError::new(error, route_source.lookup_trace.clone()),
    )?;
    route_source.lookup_trace.set_final_target(access.urc.clone());
    let children = match access.client.list(&access.urc).await {
        Ok(children) => children,
        Err(error) => {
            return Err(HpprResolveError::new(error, route_source.lookup_trace));
        },
    };
    route_source.lookup_trace.push_step(
        "listing-fetch",
        Some(access.urc.clone()),
        Some(access.endpoint.clone()),
        "hit",
        Some(format!("children={}", children.len())),
    );
    Ok(ResolvedListing {
        children,
        endpoint: access.endpoint,
        signer: access.signer,
        content_authority: access.content_authority,
        is_repo: access.is_repo,
        hppr_source: route_source.snapshot.source,
        lookup_trace: route_source.lookup_trace,
    })
}

pub async fn read_resolved_bytes(
    source: &ResolvedSourceRef,
    repo_client: &Arc<HpprdClientAsync>,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, String> {
    if source.is_repo && default_repo_backed_runtime_is_local() {
        return global_local_runtime().read_packet_bytes(&source.packet_hash, offset, length);
    }

    let client = source_client(source, repo_client)?;
    read_packet_bytes(&client, &source.packet_hash, offset, length).await
}

pub async fn resolve_embed_content_authority(
    url: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> Result<Option<String>, String> {
    let address = HAVIAddress::parse(url).map_err(|e| e.to_string())?;
    let route_source = resolve_document_source(&address, repo_client, credential_store, None)
        .await
        .map_err(|error| error.to_string())?;
    let access = resolve_access(&address, repo_client, &route_source.snapshot, address.is_listing())?;
    if access.is_repo && default_repo_backed_runtime_is_local() {
        let packet = access.client.get_packet(&access.urc).await?;
        return Ok(access
            .content_authority
            .or_else(|| seal_authority_from_urc(&access.urc))
            .or_else(|| packet_content_authority(&packet)));
    }
    Ok(access
        .content_authority
        .or_else(|| seal_authority_from_urc(&access.urc)))
}

pub async fn resolve_document_source(
    address: &HAVIAddress,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
    reuse: Option<&HpprDocumentSourceSnapshot>,
) -> Result<RouteResolvedDocumentSource, HpprResolveError> {
    if matches!(address.urc().method(), UrcMethod::Hash) {
        let request = address.urc_string();
        let mut lookup_trace = embedder_traits::HpprLookupTrace::new(
            format!("hppr:{}", request),
            "document",
        );
        lookup_trace.push_step("request", Some(request), None, "start", Some("direct hash request".to_string()));
        return Ok(RouteResolvedDocumentSource {
            snapshot: HpprDocumentSourceSnapshot {
                group: String::new(),
                app: String::new(),
                source: HpprDocumentSource::Repo,
            },
            source: RouteEndpointSource::HomeFallback,
            lookup_trace,
        });
    }

    let parts = address.parts();
    let mut lookup_trace = embedder_traits::HpprLookupTrace::new(
        format!("hppr://{}/{}/{}", parts.group, parts.app, address.location_with_slash()),
        if address.is_listing() { "listing" } else { "document" },
    );
    lookup_trace.push_step(
        "request",
        Some(HAVIAddress::build_urc_string(&parts.group, &parts.app, &address.location_with_slash())),
        None,
        "start",
        None,
    );

    if let Some(snapshot) = reuse
        && !address.has_direct_endpoint()
        && snapshot.group == parts.group
        && snapshot.app == parts.app
    {
        lookup_trace.push_step(
            "reuse-source",
            Some(format!("//{}/{}", snapshot.group, snapshot.app)),
            None,
            "hit",
            Some("reused committed page source".to_string()),
        );
        return Ok(RouteResolvedDocumentSource {
            snapshot: snapshot.clone(),
            source: match snapshot.source {
                HpprDocumentSource::Repo => RouteEndpointSource::HomeFallback,
                HpprDocumentSource::Remote { .. } => RouteEndpointSource::DirectVia,
            },
            lookup_trace,
        });
    }

    if !parts.group.starts_with('~')
        && global_state_db()
            .shadow_override_enabled(&parts.group, &parts.app)
            .map_err(|error| HpprResolveError::new(error, lookup_trace.clone()))?
    {
        lookup_trace.push_step(
            "shadow-override",
            Some(format!("//{}/{}/", parts.group, parts.app)),
            None,
            "hit",
            Some("shadow override enabled".to_string()),
        );
        return Ok(RouteResolvedDocumentSource {
            snapshot: HpprDocumentSourceSnapshot {
                group: parts.group,
                app: parts.app,
                source: HpprDocumentSource::Repo,
            },
            source: RouteEndpointSource::HomeFallback,
            lookup_trace,
        });
    }

    if let Some(endpoint) = address.endpoint_string() {
        if endpoint == "repo" {
            lookup_trace.push_step(
                "direct-via",
                Some("repo".to_string()),
                None,
                "repo",
                Some("explicit repo endpoint".to_string()),
            );
            return Ok(RouteResolvedDocumentSource {
                snapshot: HpprDocumentSourceSnapshot {
                    group: parts.group,
                    app: parts.app,
                    source: HpprDocumentSource::Repo,
                },
                source: RouteEndpointSource::HomeFallback,
                lookup_trace,
            });
        }

        let via = parse_via(&endpoint)
            .map_err(|e| HpprResolveError::new(e.to_string(), lookup_trace.clone()))?;
        lookup_trace.push_step(
            "direct-via",
            Some(format!("hppr via {}", endpoint)),
            Some(via.to_string()),
            "selected",
            None,
        );
        let route_handle = BrowserRouteHandle::new(repo_client.clone(), credential_store.clone());
        let (_, upstream_key, content_authority_pin, _) = resolve_route_endpoint_with_trace(
            &parts.group,
            &parts.app,
            &route_handle,
            &mut lookup_trace,
        )
        .await
        .map_err(|error| HpprResolveError::new(error, lookup_trace.clone()))?;
        let signer = build_route_signer(&parts.group, &parts.app, &route_handle, &mut lookup_trace)
            .await
            .map_err(|error| HpprResolveError::new(error, lookup_trace.clone()))?;
        let client = Arc::new(HpprdClientAsync::new_with_signer(via.clone(), signer.clone()));
        let content_pointer = resolve_content_pointer(
            &client,
            &parts.group,
            &parts.app,
            upstream_key.as_deref(),
            content_authority_pin.as_deref(),
            &mut lookup_trace,
        )
        .await
        .map_err(|error| HpprResolveError::new(error, lookup_trace.clone()))?;
        return Ok(RouteResolvedDocumentSource {
            snapshot: HpprDocumentSourceSnapshot {
                group: parts.group,
                app: parts.app,
                source: HpprDocumentSource::Remote {
                    endpoint: via,
                    signer,
                    content_root: content_pointer.root,
                    content_authority: content_pointer.authority,
                },
            },
            source: RouteEndpointSource::DirectVia,
            lookup_trace,
        });
    }

    let route_handle = BrowserRouteHandle::new(repo_client.clone(), credential_store.clone());
    let (endpoint, upstream_key, content_authority_pin, source) = resolve_route_endpoint_with_trace(
        &parts.group,
        &parts.app,
        &route_handle,
        &mut lookup_trace,
    )
    .await
    .map_err(|error| HpprResolveError::new(error, lookup_trace.clone()))?;
    if matches!(source, RouteEndpointSource::HomeFallback) {
        lookup_trace.push_step(
            "source-selection",
            Some(format!("//{}/{}/", parts.group, parts.app)),
            Some(route_handle.target_display()),
            "repo",
            None,
        );
        return Ok(RouteResolvedDocumentSource {
            snapshot: HpprDocumentSourceSnapshot {
                group: parts.group,
                app: parts.app,
                source: HpprDocumentSource::Repo,
            },
            source,
            lookup_trace,
        });
    }

    let signer = build_route_signer(&parts.group, &parts.app, &route_handle, &mut lookup_trace)
        .await
        .map_err(|error| HpprResolveError::new(error, lookup_trace.clone()))?;
    let client = Arc::new(HpprdClientAsync::new_with_signer(endpoint.clone(), signer.clone()));
    let content_pointer = resolve_content_pointer(
        &client,
        &parts.group,
        &parts.app,
        upstream_key.as_deref(),
        content_authority_pin.as_deref(),
        &mut lookup_trace,
    )
    .await
    .map_err(|error| HpprResolveError::new(error, lookup_trace.clone()))?;

    Ok(RouteResolvedDocumentSource {
        snapshot: HpprDocumentSourceSnapshot {
            group: parts.group,
            app: parts.app,
            source: HpprDocumentSource::Remote {
                endpoint,
                signer,
                content_root: content_pointer.root,
                content_authority: content_pointer.authority,
            },
        },
        source,
        lookup_trace,
    })
}

fn resolve_access(
    address: &HAVIAddress,
    repo_client: &Arc<HpprdClientAsync>,
    snapshot: &HpprDocumentSourceSnapshot,
    is_listing: bool,
) -> Result<ResolvedAccess, String> {
    if matches!(address.urc().method(), UrcMethod::Hash) {
        let urc = address.urc_string();
        if let Some(endpoint) = address.endpoint_string()
            && endpoint != "repo"
        {
            let via = parse_via(&endpoint).map_err(|error| error.to_string())?;
            let signer = Signer::anyone();
            let client = Arc::new(HpprdClientAsync::new_with_signer(via.clone(), signer.clone()));
            return Ok(ResolvedAccess {
                endpoint: via.to_string(),
                signer: Some(signer),
                content_authority: seal_authority_from_urc(&urc),
                is_repo: false,
                client: ResolvedClient::Remote(client),
                urc,
            });
        }
        let client = if default_repo_backed_runtime_is_local() {
            ResolvedClient::Local(global_local_runtime())
        } else {
            ResolvedClient::Remote(repo_client.clone())
        };
        return Ok(ResolvedAccess {
            endpoint: if default_repo_backed_runtime_is_local() {
                "repo".to_string()
            } else {
                repo_client.target().to_string()
            },
            signer: None,
            content_authority: seal_authority_from_urc(&urc),
            is_repo: true,
            client,
            urc,
        });
    }

    let parts = address.parts();
    let requested_location = address.location_with_slash();

    match &snapshot.source {
        HpprDocumentSource::Repo => {
            let urc = if !parts.group.starts_with('~')
                && global_state_db().shadow_override_enabled(&parts.group, &parts.app)?
            {
                let target = append_location(&shadow_root(&parts.group, &parts.app), &requested_location);
                if is_listing {
                    format!("{}/", target.trim_end_matches('/'))
                } else {
                    target
                }
            } else {
                HAVIAddress::build_urc_string(&parts.group, &parts.app, &requested_location)
            };
            let client = if default_repo_backed_runtime_is_local() {
                ResolvedClient::Local(global_local_runtime())
            } else {
                ResolvedClient::Remote(repo_client.clone())
            };
            Ok(ResolvedAccess {
                endpoint: if default_repo_backed_runtime_is_local() {
                    "repo".to_string()
                } else {
                    repo_client.target().to_string()
                },
                signer: None,
                content_authority: None,
                is_repo: true,
                client,
                urc,
            })
        },
        HpprDocumentSource::Remote {
            endpoint,
            signer,
            content_root,
            content_authority,
        } => {
            let client = Arc::new(HpprdClientAsync::new_with_signer(
                endpoint.clone(),
                signer.clone(),
            ));
            let target = append_location(content_root, &requested_location);
            let urc = if is_listing {
                format!("{}/", target.trim_end_matches('/'))
            } else {
                format!("{}/|/seal/{}", target, content_authority)
            };
            Ok(ResolvedAccess {
                endpoint: endpoint.to_string(),
                signer: Some(signer.clone()),
                content_authority: Some(content_authority.clone()),
                is_repo: false,
                client: ResolvedClient::Remote(client),
                urc,
            })
        },
    }
}

async fn resolve_content_pointer(
    route_client: &Arc<HpprdClientAsync>,
    group: &str,
    app: &str,
    upstream_key: Option<&str>,
    network_content_authority_pin: Option<&str>,
    lookup_trace: &mut embedder_traits::HpprLookupTrace,
) -> Result<ContentPointerInfo, String> {
    let repo_vkey = match upstream_key {
        Some(key) => key.to_string(),
        None => route_client.get_admin_identity().await?,
    };

    let deploy_urc = format!("//{group}/admin/deploy/{app}/|/seal/{repo_vkey}");
    let content_pointer = match route_client.get_content_pointer(group, app, &repo_vkey).await {
        Ok(content_pointer) => content_pointer,
        Err(error) => {
            lookup_trace.push_step(
                "deploy-pointer",
                Some(deploy_urc),
                Some(route_client.target().to_string()),
                "error",
                Some(error.clone()),
            );
            return Err(error);
        },
    };

    lookup_trace.push_step(
        "deploy-pointer",
        Some(deploy_urc.clone()),
        Some(route_client.target().to_string()),
        "hit",
        Some(format!(
            "Content-Root={} Content-Authority={}",
            content_pointer.root, content_pointer.authority
        )),
    );

    if let Some(pin) = network_content_authority_pin {
        if content_pointer.authority != pin {
            let detail = format!(
                "Content-Authority mismatch: network pin {} != deploy pointer {}",
                pin, content_pointer.authority
            );
            lookup_trace.push_step(
                "content-authority-pin",
                Some(deploy_urc.clone()),
                None,
                "mismatch",
                Some(detail.clone()),
            );
            return Err(detail);
        }
        lookup_trace.push_step(
            "content-authority-pin",
            Some(pin.to_string()),
            None,
            "match",
            None,
        );
    }

    Ok(content_pointer)
}

async fn build_route_signer(
    group: &str,
    app: &str,
    handle: &BrowserRouteHandle,
    lookup_trace: &mut embedder_traits::HpprLookupTrace,
) -> Result<Signer, String> {
    let repo_vkey = match handle.admin_identity().await {
        Some(vkey) => vkey,
        None => {
            lookup_trace.push_step(
                "route-auth",
                Some("//repo/admin/identity/|".to_string()),
                Some(handle.target_display()),
                "fallback",
                Some("missing admin identity; using anyone".to_string()),
            );
            return Ok(Signer::anyone());
        },
    };
    let route_auth = match handle.route_auth(group, Some(app), &repo_vkey).await {
        Ok(info) => info,
        Err(_) => {
            lookup_trace.push_step(
                "route-auth",
                Some(format!("//repo/route/auth/{}/{}/|/seal/{}", group, app, repo_vkey)),
                Some(handle.target_display()),
                "fallback",
                Some("route auth missing; using anyone".to_string()),
            );
            return Ok(Signer::anyone());
        },
    };
    lookup_trace.push_step(
        "route-auth",
        Some(format!("//repo/route/auth/{}/{}/|/seal/{}", group, app, repo_vkey)),
        Some(handle.target_display()),
        "hit",
        Some(route_auth.auth.clone()),
    );
    let signer = Signer::parse(&route_auth.auth).map_err(|e| e.to_string())?;
    match signer {
        Signer::Ring2Contextual { .. } => signer.derive_for(group).map_err(|e| e.to_string()),
        other => Ok(other),
    }
}

fn source_client(
    source: &ResolvedSourceRef,
    repo_client: &Arc<HpprdClientAsync>,
) -> Result<Arc<HpprdClientAsync>, String> {
    if source.is_repo {
        return Ok(repo_client.clone());
    }

    let signer = source
        .signer
        .clone()
        .ok_or("resolved source missing signer for routed access")?;
    let endpoint = parse_via(&source.endpoint).map_err(|error| error.to_string())?;
    Ok(Arc::new(HpprdClientAsync::new_with_signer(
        endpoint,
        signer,
    )))
}

async fn read_packet_bytes(
    client: &Arc<HpprdClientAsync>,
    packet_hash: &str,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, String> {
    if length == 0 {
        return Ok(Vec::new());
    }

    let packet = client
        .get_packet_authenticated(&format!("////{packet_hash}"))
        .await?;
    let headers = packet_headers(&packet);

    if is_chunk_manifest(&headers) {
        let manifest = parse_chunk_manifest(&headers)
            .map_err(|e| format!("invalid chunk manifest: {e}"))?;
        return Box::pin(read_manifest_bytes(client, &manifest, offset, length)).await;
    }

    Ok(slice_bytes(packet.data(), offset, length).to_vec())
}

async fn read_manifest_bytes(
    client: &Arc<HpprdClientAsync>,
    manifest: &ChunkManifest,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, String> {
    let Some((start, end)) = clamp_range(manifest.total_length, offset, length) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::with_capacity((end - start) as usize);
    for chunk in &manifest.chunks {
        if chunk.end <= start || chunk.start >= end {
            continue;
        }

        let read_start = start.max(chunk.start);
        let read_end = end.min(chunk.end);
        let local_offset = read_start - chunk.start;
        let local_len = (read_end - read_start) as usize;
        let bytes = match chunk.kind {
            ChunkKind::Blob => read_blob_chunk_bytes(client, &chunk.hash, local_offset, local_len).await?,
            ChunkKind::Manifest => read_packet_bytes(client, &chunk.hash, local_offset, local_len).await?,
        };
        if bytes.len() != local_len {
            return Err(format!(
                "chunk {} returned {} bytes, expected {}",
                chunk.hash,
                bytes.len(),
                local_len,
            ));
        }
        out.extend_from_slice(&bytes);
    }

    Ok(out)
}

async fn read_blob_chunk_bytes(
    client: &Arc<HpprdClientAsync>,
    hash: &str,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, String> {
    let packet = client
        .get_packet_authenticated(&format!("////{hash}"))
        .await?;
    Ok(slice_bytes(packet.data(), offset, length).to_vec())
}

fn packet_headers(packet: &Packet) -> Vec<(String, String)> {
    packet
        .headers()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn packet_content_authority(packet: &Packet) -> Option<String> {
    packet.header("Seal-By").map(str::to_string)
}

fn seal_authority_from_urc(urc: &str) -> Option<String> {
    let (_, rest) = urc.split_once("/|/seal/")?;
    let signer = rest.split('/').next()?;
    if signer.starts_with("V.") && signer.ends_with(".H3") {
        Some(signer.to_string())
    } else {
        None
    }
}

fn clamp_range(total: u64, offset: u64, length: usize) -> Option<(u64, u64)> {
    if length == 0 || offset >= total {
        return None;
    }
    let end = offset.saturating_add(length as u64).min(total);
    Some((offset, end))
}

fn slice_bytes(bytes: &[u8], offset: u64, length: usize) -> &[u8] {
    let Some((start, end)) = clamp_range(bytes.len() as u64, offset, length) else {
        return &[];
    };
    &bytes[start as usize..end as usize]
}
