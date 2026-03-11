/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR-backed baked media assets.
//!
//! This layer stays in HAVI/browser code. It resolves HPPR packet content into
//! a browser-owned media asset plus a blocking byte source that the media crate
//! can consume without learning about HPPR routing, auth, or chunk manifests.

use std::fmt;
use std::sync::{Arc, Mutex};

use hppr_client::ViaSpec;
use hppr_packet::Packet;
use hppr_segment::chunk::{ChunkManifest, is_chunk_manifest, parse_chunk_manifest};
use hppr_segment::chunk_loader::{ChunkLoader, LoaderConfig};
use media::{MediaAssetMetadata, MediaByteSource, ResolvedMediaAsset, clamp_byte_range};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HpprResolvedMediaKind {
    Blob,
    ChunkManifest,
}

/// HPPR-specific baked media asset metadata retained above the shared
/// `MediaByteSource` boundary.
#[derive(Clone)]
pub struct ResolvedHpprMediaAsset {
    kind: HpprResolvedMediaKind,
    endpoint: ViaSpec,
    is_repo: bool,
    packet_hash: String,
    asset: ResolvedMediaAsset,
}

impl ResolvedHpprMediaAsset {
    /// Resolve a fetched HPPR packet into a browser-owned baked media asset.
    ///
    /// `chunk_fetcher` stays in HAVI and must return blob bytes for `B.*`
    /// hashes and full packet bytes for nested `P.*` manifest hashes.
    pub fn from_packet(
        endpoint: ViaSpec,
        is_repo: bool,
        packet: &Packet,
        chunk_fetcher: Arc<dyn Fn(&str) -> Result<Vec<u8>, String> + Send + Sync>,
    ) -> Result<Self, String> {
        let packet_hash = packet.pkt_hash().to_string();
        let headers = packet_headers(packet);

        if is_chunk_manifest(&headers) {
            let manifest = parse_chunk_manifest(&headers)
                .map_err(|e| format!("invalid chunk manifest: {e}"))?;
            let content_type = manifest.content_type.clone();
            let content_length = manifest.total_length;
            let byte_source = Arc::new(ChunkManifestByteSource::new(manifest, chunk_fetcher));
            let asset = ResolvedMediaAsset::new(
                MediaAssetMetadata::new(content_length, content_type),
                byte_source,
            );
            return Ok(Self {
                kind: HpprResolvedMediaKind::ChunkManifest,
                endpoint,
                is_repo,
                packet_hash,
                asset,
            });
        }

        let content_type = packet.header("Content-Type").map(str::to_string);
        let content_length = packet.data().len() as u64;
        let byte_source = Arc::new(InMemoryByteSource::new(packet.data().to_vec()));
        let asset = ResolvedMediaAsset::new(
            MediaAssetMetadata::new(content_length, content_type),
            byte_source,
        );
        Ok(Self {
            kind: HpprResolvedMediaKind::Blob,
            endpoint,
            is_repo,
            packet_hash,
            asset,
        })
    }

    pub fn kind(&self) -> &HpprResolvedMediaKind {
        &self.kind
    }

    pub fn endpoint(&self) -> &ViaSpec {
        &self.endpoint
    }

    pub fn is_repo(&self) -> bool {
        self.is_repo
    }

    pub fn packet_hash(&self) -> &str {
        &self.packet_hash
    }

    pub fn asset(&self) -> &ResolvedMediaAsset {
        &self.asset
    }

    pub fn into_asset(self) -> ResolvedMediaAsset {
        self.asset
    }

    pub fn read_range(&self, start: u64, len: usize) -> Result<Vec<u8>, String> {
        self.asset.read_range(start, len)
    }
}

impl fmt::Debug for ResolvedHpprMediaAsset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedHpprMediaAsset")
            .field("kind", &self.kind)
            .field("endpoint", &self.endpoint)
            .field("is_repo", &self.is_repo)
            .field("packet_hash", &self.packet_hash)
            .field("asset", &self.asset)
            .finish()
    }
}

fn packet_headers(packet: &Packet) -> Vec<(String, String)> {
    packet
        .headers()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

struct InMemoryByteSource {
    bytes: Vec<u8>,
}

impl InMemoryByteSource {
    fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

impl MediaByteSource for InMemoryByteSource {
    fn read_range(&self, start: u64, len: usize) -> Result<Vec<u8>, String> {
        let Some((start, end)) = clamp_byte_range(self.bytes.len() as u64, start, len) else {
            return Ok(Vec::new());
        };
        Ok(self.bytes[start as usize..end as usize].to_vec())
    }
}

struct ChunkManifestByteSource {
    manifest: ChunkManifest,
    loader: Mutex<ChunkLoader>,
    chunk_fetcher: Arc<dyn Fn(&str) -> Result<Vec<u8>, String> + Send + Sync>,
}

impl ChunkManifestByteSource {
    fn new(
        manifest: ChunkManifest,
        chunk_fetcher: Arc<dyn Fn(&str) -> Result<Vec<u8>, String> + Send + Sync>,
    ) -> Self {
        let loader = ChunkLoader::new(
            manifest.clone(),
            LoaderConfig {
                cache_capacity: 16,
                prefetch_ahead: 2,
                verify_full_hash: true,
            },
        );
        Self {
            manifest,
            loader: Mutex::new(loader),
            chunk_fetcher,
        }
    }
}

impl MediaByteSource for ChunkManifestByteSource {
    fn read_range(&self, start: u64, len: usize) -> Result<Vec<u8>, String> {
        let Some((start, end)) = clamp_byte_range(self.manifest.total_length, start, len) else {
            return Ok(Vec::new());
        };

        let mut loader = self.loader.lock().unwrap();
        let mut fetch = |hash: &str| (self.chunk_fetcher)(hash).map_err(anyhow::Error::msg);
        let mut cursor = start;
        let mut out = Vec::with_capacity((end - start) as usize);

        while cursor < end {
            let (chunk, offset_in_chunk) = loader
                .fetch_at(cursor, &mut fetch)
                .map_err(|e| e.to_string())?;
            let remaining = (end - cursor) as usize;
            let available = chunk.len().saturating_sub(offset_in_chunk);
            let take = remaining.min(available);
            if take == 0 {
                return Err("chunk loader returned an empty readable range".into());
            }
            out.extend_from_slice(&chunk[offset_in_chunk..offset_in_chunk + take]);
            cursor = cursor.saturating_add(take as u64);
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hppr_client::parse_via;
    use hppr_packet::crypto::calculate_hash;
    use hppr_packet::writer::PacketWriter;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn endpoint() -> ViaSpec {
        parse_via("127.0.0.1:4777").unwrap()
    }

    fn build_blob_packet(data: &[u8], content_type: Option<&str>) -> Packet {
        let mut writer = if let Some(content_type) = content_type {
            PacketWriter::plex_with_headers(
                "g",
                "app",
                "movie.mp4",
                "1234567890:123456789",
                &[("Content-Type", content_type)],
            )
            .unwrap()
        } else {
            PacketWriter::plex("g", "app", "movie.mp4", "1234567890:123456789").unwrap()
        };
        writer.write_data(data).unwrap();
        let (bytes, _) = writer.finish().unwrap();
        Packet::parse(bytes.into_boxed_slice()).unwrap()
    }

    fn build_manifest_packet(
        chunks: &[(u64, u64, &str)],
        total_length: u64,
        content_type: Option<&str>,
    ) -> Packet {
        let mut owned_headers: Vec<(String, String)> = chunks
            .iter()
            .map(|(start, end, hash)| ("Chunk+Link".to_string(), format!("{start}..{end} {hash}")))
            .collect();
        owned_headers.push((
            "Content-Total-Length".to_string(),
            total_length.to_string(),
        ));
        if let Some(content_type) = content_type {
            owned_headers.push(("Content-Type".to_string(), content_type.to_string()));
        }
        let header_refs: Vec<(&str, &str)> = owned_headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let mut writer = PacketWriter::plex_with_headers(
            "g",
            "app",
            "movie.mp4",
            "1234567890:123456789",
            &header_refs,
        )
        .unwrap();
        let (bytes, _) = writer.finish().unwrap();
        Packet::parse(bytes.into_boxed_slice()).unwrap()
    }

    fn manifest_packet_bytes(
        chunks: &[(u64, u64, &str)],
        total_length: u64,
        content_type: Option<&str>,
    ) -> Vec<u8> {
        let packet = build_manifest_packet(chunks, total_length, content_type);
        packet.as_bytes().to_vec()
    }

    fn blob_hash(data: &[u8]) -> String {
        calculate_hash('B', [data])
    }

    #[test]
    fn blob_asset_reads_clamped_ranges() {
        let packet = build_blob_packet(b"abcdefghij", Some("video/mp4"));
        let asset = ResolvedHpprMediaAsset::from_packet(endpoint(), true, &packet, Arc::new(|_| {
            Err("unused".into())
        }))
        .unwrap();

        assert_eq!(asset.kind(), &HpprResolvedMediaKind::Blob);
        assert!(asset.is_repo());
        assert_eq!(asset.asset().content_type(), Some("video/mp4"));
        assert_eq!(asset.read_range(2, 4).unwrap(), b"cdef");
        assert_eq!(asset.read_range(8, 10).unwrap(), b"ij");
        assert!(asset.read_range(10, 4).unwrap().is_empty());
    }

    #[test]
    fn chunk_manifest_asset_reads_random_ranges() {
        let c1 = b"abcd";
        let c2 = b"efgh";
        let c3 = b"ijkl";
        let h1 = blob_hash(c1);
        let h2 = blob_hash(c2);
        let h3 = blob_hash(c3);
        let packet = build_manifest_packet(
            &[(0, 4, &h1), (4, 8, &h2), (8, 12, &h3)],
            12,
            Some("video/mp4"),
        );

        let blobs = HashMap::from([
            (h1.clone(), c1.to_vec()),
            (h2.clone(), c2.to_vec()),
            (h3.clone(), c3.to_vec()),
        ]);
        let fetches = Arc::new(AtomicUsize::new(0));
        let fetches_clone = fetches.clone();
        let asset = ResolvedHpprMediaAsset::from_packet(
            endpoint(),
            false,
            &packet,
            Arc::new(move |hash| {
                fetches_clone.fetch_add(1, Ordering::Relaxed);
                blobs
                    .get(hash)
                    .cloned()
                    .ok_or_else(|| format!("missing chunk {hash}"))
            }),
        )
        .unwrap();

        assert_eq!(asset.kind(), &HpprResolvedMediaKind::ChunkManifest);
        assert!(!asset.is_repo());
        assert_eq!(asset.asset().content_type(), Some("video/mp4"));
        assert_eq!(asset.asset().content_length(), 12);
        assert_eq!(asset.read_range(1, 7).unwrap(), b"bcdefgh");
        assert_eq!(asset.read_range(6, 4).unwrap(), b"ghij");
        assert_eq!(asset.read_range(0, 12).unwrap(), b"abcdefghijkl");
        assert!(fetches.load(Ordering::Relaxed) >= 3);
    }

    #[test]
    fn chunk_manifest_asset_resolves_nested_manifests() {
        let inner_chunk = b"nested-content";
        let inner_hash = blob_hash(inner_chunk);
        let nested_manifest_bytes = manifest_packet_bytes(
            &[(0, inner_chunk.len() as u64, &inner_hash)],
            inner_chunk.len() as u64,
            None,
        );
        let nested_manifest_hash = calculate_hash('P', [nested_manifest_bytes.as_slice()]);
        let packet = build_manifest_packet(
            &[(0, inner_chunk.len() as u64, &nested_manifest_hash)],
            inner_chunk.len() as u64,
            Some("audio/mp4"),
        );

        let blobs = HashMap::from([
            (inner_hash.clone(), inner_chunk.to_vec()),
            (nested_manifest_hash.clone(), nested_manifest_bytes),
        ]);
        let asset = ResolvedHpprMediaAsset::from_packet(
            endpoint(),
            false,
            &packet,
            Arc::new(move |hash| {
                blobs
                    .get(hash)
                    .cloned()
                    .ok_or_else(|| format!("missing chunk {hash}"))
            }),
        )
        .unwrap();

        assert_eq!(asset.asset().content_type(), Some("audio/mp4"));
        assert_eq!(asset.read_range(0, inner_chunk.len()).unwrap(), inner_chunk);
        assert_eq!(asset.read_range(7, 7).unwrap(), b"content");
    }
}
