/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared HPPR source resolution.
//!
//! This module owns browser-level HPPR resolution policy:
//! - route selection
//! - content-pointer resolution
//! - endpoint and signer choice
//! - document/media packet resolution
//! - source-based byte reads

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use hppr_client::{Packet, Signer, ViaSpec, parse_via};
use hppr_packet::chunk::{ChunkKind, ChunkManifest, is_chunk_manifest, parse_chunk_manifest};

use super::client::HpprdClientAsync;
use super::credentials::CredentialStoreHandle;
use super::url::HAVIAddress;
use super::state_db::global_state_db;
use super::util::{RouteEndpointSource, append_location, resolve_route_endpoint, shadow_root};

#[derive(Clone, Debug)]
pub struct ResolvedSourceRef {
    pub endpoint: ViaSpec,
    pub signer: Option<Signer>,
    pub content_authority: Option<String>,
    pub packet_hash: String,
    pub is_repo: bool,
}

#[derive(Clone, Debug)]
pub struct ResolvedDocument {
    pub packet: Packet,
    pub endpoint: ViaSpec,
    pub signer: Option<Signer>,
    pub content_authority: Option<String>,
    pub is_repo: bool,
    pub source: ResolvedSourceRef,
}

#[derive(Clone, Debug)]
pub struct ResolvedMediaSource {
    pub packet: Packet,
    pub endpoint: ViaSpec,
    pub signer: Option<Signer>,
    pub content_authority: Option<String>,
    pub is_repo: bool,
    pub source: ResolvedSourceRef,
}

#[derive(Clone, Debug)]
pub struct ResolvedListing {
    pub children: Vec<String>,
    pub endpoint: ViaSpec,
    pub signer: Option<Signer>,
    pub content_authority: Option<String>,
    pub is_repo: bool,
}

#[derive(Clone, Debug)]
struct ResolvedTarget {
    endpoint: ViaSpec,
    upstream_key: Option<String>,
    content_authority_pin: Option<String>,
    source: RouteEndpointSource,
}

struct ResolvedAccess {
    endpoint: ViaSpec,
    signer: Option<Signer>,
    content_authority: Option<String>,
    is_repo: bool,
    client: Arc<HpprdClientAsync>,
    urc: String,
}

static PACKET_CACHE: LazyLock<Mutex<HashMap<String, Arc<Vec<u8>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub async fn route_configured_for_direct_endpoint(
    group: &str,
    app: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> bool {
    if group.is_empty() || app.is_empty() || credential_store.get_admin().is_none() {
        return false;
    }

    let Ok(repo_vkey) = repo_client.get_admin_identity().await else {
        return false;
    };

    repo_client.get_route(group, app, &repo_vkey).await.is_ok()
}

pub async fn resolve_document(
    url: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> Result<ResolvedDocument, String> {
    let address = HAVIAddress::parse(url).map_err(|e| e.to_string())?;
    if address.is_listing() {
        return Err("listing URL cannot resolve to a document source".to_string());
    }

    let access = resolve_access(&address, repo_client, credential_store, false).await?;
    let packet = access.client.get_packet_authenticated(&access.urc).await?;
    let content_authority = access.content_authority.or_else(|| packet_content_authority(&packet));
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
        source,
    })
}

pub async fn resolve_media(
    url: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> Result<ResolvedMediaSource, String> {
    let address = HAVIAddress::parse(url).map_err(|e| e.to_string())?;
    if address.is_listing() {
        return Err("listing URL cannot resolve to a media source".to_string());
    }

    let access = resolve_access(&address, repo_client, credential_store, false).await?;
    let packet = access.client.get_packet_authenticated(&access.urc).await?;
    let content_authority = access.content_authority.or_else(|| packet_content_authority(&packet));
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
        source,
    })
}

pub async fn resolve_listing(
    url: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> Result<ResolvedListing, String> {
    let address = HAVIAddress::parse(url).map_err(|e| e.to_string())?;
    let access = resolve_access(&address, repo_client, credential_store, true).await?;
    let children = access.client.list(&access.urc).await?;
    Ok(ResolvedListing {
        children,
        endpoint: access.endpoint,
        signer: access.signer,
        content_authority: access.content_authority,
        is_repo: access.is_repo,
    })
}

pub async fn read_resolved_bytes(
    source: &ResolvedSourceRef,
    repo_client: &Arc<HpprdClientAsync>,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, String> {
    let client = source_client(source, repo_client)?;
    read_packet_bytes(&client, &source.packet_hash, offset, length).await
}

pub async fn resolve_embed_content_authority(
    url: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> Result<Option<String>, String> {
    let address = HAVIAddress::parse(url).map_err(|e| e.to_string())?;
    let access = resolve_access(&address, repo_client, credential_store, address.is_listing()).await?;
    Ok(access
        .content_authority
        .or_else(|| seal_authority_from_urc(&access.urc)))
}

async fn resolve_access(
    address: &HAVIAddress,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
    is_listing: bool,
) -> Result<ResolvedAccess, String> {
    let parts = address.parts();
    if !parts.group.starts_with('~')
        && global_state_db().shadow_override_enabled(&parts.group, &parts.app)?
    {
        let requested_location = address.location_with_slash();
        let target = append_location(&shadow_root(&parts.group, &parts.app), &requested_location);
        let urc = if is_listing {
            format!("{}/", target.trim_end_matches('/'))
        } else {
            target
        };
        return Ok(ResolvedAccess {
            endpoint: repo_client.target(),
            signer: None,
            content_authority: None,
            is_repo: true,
            client: repo_client.clone(),
            urc,
        });
    }

    let target = resolve_target(address, repo_client, credential_store, None).await?;
    let is_repo = matches!(target.source, RouteEndpointSource::HomeFallback);
    let network_content_authority_pin = target.content_authority_pin.clone();
    let signer = if is_repo {
        None
    } else {
        Some(build_route_signer(&parts.group, repo_client, credential_store).await?)
    };
    let client = if let Some(signer) = &signer {
        Arc::new(HpprdClientAsync::new_with_signer(
            target.endpoint.clone(),
            signer.clone(),
        ))
    } else {
        repo_client.clone()
    };

    let requested_location = address.location_with_slash();
    let (urc, content_authority) = if is_repo {
        (
            HAVIAddress::build_urc_string(&parts.group, &parts.app, &requested_location),
            None,
        )
    } else {
        resolve_content_pointer_target(
            &client,
            &parts.group,
            &parts.app,
            &requested_location,
            target.upstream_key.as_deref(),
            is_listing,
            network_content_authority_pin.as_deref(),
        )
        .await?
    };

    Ok(ResolvedAccess {
        endpoint: target.endpoint,
        signer,
        content_authority,
        is_repo,
        client,
        urc,
    })
}

async fn resolve_target(
    url: &HAVIAddress,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
    page_endpoint: Option<&ViaSpec>,
) -> Result<ResolvedTarget, String> {
    let repo_target = repo_client.target();
    let parts = url.parts();

    let (endpoint, upstream_key, content_authority_pin, source) = if let Some(endpoint) = url.endpoint_string() {
        if endpoint == "repo" {
            (repo_target, None, None, RouteEndpointSource::HomeFallback)
        } else {
            let via = parse_via(&endpoint).map_err(|e| e.to_string())?;
            eprintln!(
                "[havi] route resolve: //{}/{} source={} endpoint={}",
                parts.group,
                parts.app,
                RouteEndpointSource::DirectVia.as_str(),
                via
            );
            (via, None, None, RouteEndpointSource::DirectVia)
        }
    } else if let Some(endpoint) = page_endpoint {
        eprintln!(
            "[havi] route resolve: //{}/{} source={} endpoint={}",
            parts.group,
            parts.app,
            RouteEndpointSource::ParentRoute.as_str(),
            endpoint
        );
        (endpoint.clone(), None, None, RouteEndpointSource::ParentRoute)
    } else {
        resolve_route_endpoint(&parts.group, &parts.app, repo_client, credential_store).await?
    };

    Ok(ResolvedTarget {
        endpoint,
        upstream_key,
        content_authority_pin,
        source,
    })
}

async fn resolve_content_pointer_target(
    route_client: &Arc<HpprdClientAsync>,
    group: &str,
    app: &str,
    requested_location: &str,
    upstream_key: Option<&str>,
    is_listing: bool,
    network_content_authority_pin: Option<&str>,
) -> Result<(String, Option<String>), String> {
    let repo_vkey = match upstream_key {
        Some(key) => key.to_string(),
        None => route_client.get_admin_identity().await?,
    };

    let content_pointer = route_client.get_content_pointer(group, app, &repo_vkey).await?;

    // MUST enforce Content-Authority pin from public network against deploy pointer
    if let Some(pin) = network_content_authority_pin {
        if content_pointer.authority != pin {
            return Err(format!(
                "Content-Authority mismatch: network pin {} != deploy pointer {}",
                pin, content_pointer.authority
            ));
        }
    }

    let target = append_location(&content_pointer.root, requested_location);
    let urc = if is_listing {
        format!("{}/", target.trim_end_matches('/'))
    } else {
        format!("{}/|/seal/{}", target, content_pointer.authority)
    };
    Ok((urc, Some(content_pointer.authority)))
}

async fn build_route_signer(
    group: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> Result<Signer, String> {
    let route_cred = credential_store
        .get_or_create_route_credential_async(group, repo_client)
        .await?;
    Ok(Signer::ring2(group, route_cred.signing_key()))
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
    Ok(Arc::new(HpprdClientAsync::new_with_signer(
        source.endpoint.clone(),
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

    let packet = get_cached_packet(client, packet_hash).await?;
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
    let packet = get_cached_packet(client, hash).await?;
    Ok(slice_bytes(packet.data(), offset, length).to_vec())
}

async fn get_cached_packet(
    client: &Arc<HpprdClientAsync>,
    packet_hash: &str,
) -> Result<Packet, String> {
    if let Some(bytes) = PACKET_CACHE.lock().unwrap().get(packet_hash).cloned() {
        return Packet::parse(bytes.as_slice().to_vec().into_boxed_slice())
            .map_err(|error| format!("cached packet parse failed for {packet_hash}: {error}"));
    }

    let packet = client
        .get_packet_authenticated(&format!("////{packet_hash}"))
        .await?;
    PACKET_CACHE
        .lock()
        .unwrap()
        .insert(packet_hash.to_string(), Arc::new(packet.as_bytes().to_vec()));
    Ok(packet)
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
