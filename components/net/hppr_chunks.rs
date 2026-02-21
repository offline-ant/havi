/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared chunk reassembly logic for HPPR.
//!
//! Used by both the protocol handler (servoshell) and the resource thread (net)
//! to fetch chunk blobs via EXCHANGE and reassemble them with ChunkLoader.

use std::collections::HashMap;
use std::sync::Arc;

use hppr_client::env_target::ViaSpec;
use hppr_client::ExchangeItem;
use hppr_client::{HpprRequest as IoRequest, ResponseKind, Signer};
use hppr_packet::chunk::ChunkManifest;
use hppr_packet::chunk_loader::{ChunkLoader, LoaderConfig};

use crate::hppr_pool::HpprAsyncState;

/// Batch fetch chunk blobs via EXCHANGE, then reassemble with ChunkLoader.
///
/// Tries repo first, then route for missing blobs (when endpoint != repo).
pub async fn batch_reassemble_chunks(
    hppr_state: &Arc<HpprAsyncState>,
    endpoint: &ViaSpec,
    is_repo: bool,
    manifest: &ChunkManifest,
) -> Result<Vec<u8>, String> {
    let all_hashes: Vec<&str> = manifest.chunks.iter().map(|c| c.hash.as_str()).collect();
    if all_hashes.is_empty() {
        return Ok(Vec::new());
    }

    let mut blobs = fetch_chunk_blobs(hppr_state, endpoint, is_repo, &all_hashes).await?;

    let config = LoaderConfig {
        cache_capacity: 0,
        prefetch_ahead: 0,
        verify_full_hash: true,
    };
    let mut loader = ChunkLoader::new(manifest.clone(), config);
    loader
        .collect_all(&mut |hash| {
            blobs
                .remove(hash)
                .ok_or_else(|| anyhow::anyhow!("chunk blob not found: {hash}"))
        })
        .map_err(|e| format!("chunk reassembly: {e}"))
}

/// Fetch chunk blobs by hash via EXCHANGE (repo first, route fallback).
///
/// Returns a hash->data map. Auto-caches route-fetched blobs to home repo.
pub async fn fetch_chunk_blobs(
    hppr_state: &Arc<HpprAsyncState>,
    endpoint: &ViaSpec,
    is_repo: bool,
    hashes: &[&str],
) -> Result<HashMap<String, Vec<u8>>, String> {
    if hashes.is_empty() {
        return Ok(HashMap::new());
    }

    let items: Vec<ExchangeItem> = hashes
        .iter()
        .map(|h| ExchangeItem::Need(format!("////{h}")))
        .collect();

    // EXCHANGE on repo
    let repo_target = &hppr_state.default_target;
    let repo_pooled = hppr_state
        .get_pooled(repo_target, Signer::anyone())
        .await
        .map_err(|e| format!("chunk exchange connect (repo): {e}"))?;
    let repo_resp = repo_pooled
        .connection()
        .send(IoRequest::Exchange { items: items.clone() })
        .await
        .map_err(|e| format!("chunk exchange (repo): {e}"))?;

    let mut blobs: HashMap<String, Vec<u8>> = HashMap::new();
    if let ResponseKind::Exchange(result) = repo_resp.kind {
        parse_exchange_into_blobs(&result.received, &mut blobs)?;
    }

    // Route fallback for missing hashes
    let missing: Vec<&str> = hashes
        .iter()
        .filter(|h| !blobs.contains_key(**h))
        .copied()
        .collect();
    if !missing.is_empty() && !is_repo {
        let missing_items: Vec<ExchangeItem> = missing
            .iter()
            .map(|h| ExchangeItem::Need(format!("////{h}")))
            .collect();
        let route_pooled = hppr_state
            .get_pooled(endpoint, Signer::anyone())
            .await
            .map_err(|e| format!("chunk exchange connect (route): {e}"))?;
        let route_resp = route_pooled
            .connection()
            .send(IoRequest::Exchange { items: missing_items })
            .await
            .map_err(|e| format!("chunk exchange (route): {e}"))?;
        if let ResponseKind::Exchange(result) = route_resp.kind {
            // Auto-cache: STORE route-fetched chunk blobs to home repo
            for raw in &result.received {
                let state = Arc::clone(hppr_state);
                let cache_bytes = raw.clone();
                tokio::spawn(async move {
                    if let Ok(p) = state.get_pooled(&state.default_target, Signer::anyone()).await {
                        let _ = p
                            .connection()
                            .send(IoRequest::Store { packet: cache_bytes })
                            .await;
                    }
                });
            }
            parse_exchange_into_blobs(&result.received, &mut blobs)?;
        }
    }

    Ok(blobs)
}

/// Parse EXCHANGE received packets into hash -> data map.
///
/// For B-type hashes, stores blob data. For P-type hashes, stores raw packet
/// bytes (ChunkLoader needs full packet bytes to parse nested manifests).
pub fn parse_exchange_into_blobs(
    received: &[Vec<u8>],
    blobs: &mut HashMap<String, Vec<u8>>,
) -> Result<(), String> {
    for raw in received {
        let pkt = hppr_packet::read_packet(raw.clone().into_boxed_slice())
            .map_err(|e| format!("failed to parse received chunk packet: {e}"))?;
        let hash = pkt.hash_header();
        let hash = hash
            .strip_prefix("🖧: ")
            .and_then(|h| h.strip_suffix('\n'))
            .unwrap_or(hash);
        let value = if hash.starts_with("P.") {
            pkt.as_bytes().to_vec()
        } else {
            pkt.data().to_vec()
        };
        blobs.insert(hash.to_string(), value);
    }
    Ok(())
}
