/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR-backed resolved media assets.
//!
//! This layer stays in HAVI/browser code. It resolves HPPR packet content into
//! a browser-owned media asset plus a blocking byte source that the media crate
//! can consume without learning about HPPR routing, auth, or chunk manifests.

use std::fmt;
use std::sync::Arc;

use hppr_packet::Packet;
use hppr_packet::chunk::{is_chunk_manifest, parse_chunk_manifest};
use crate::media::{MediaAssetMetadata, MediaByteSource, ResolvedMediaAsset, clamp_byte_range};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HpprResolvedMediaKind {
    Blob,
    ChunkManifest,
}

/// HPPR-specific resolved media asset metadata retained above the shared
/// `MediaByteSource` boundary.
#[derive(Clone)]
pub struct ResolvedHpprMediaAsset {
    kind: HpprResolvedMediaKind,
    endpoint: String,
    is_repo: bool,
    packet_hash: String,
    asset: ResolvedMediaAsset,
}

impl ResolvedHpprMediaAsset {
    /// Resolve a fetched HPPR packet into a browser-owned resolved media asset.
    pub fn from_packet(
        endpoint: String,
        is_repo: bool,
        packet: &Packet,
        read_range: Arc<dyn Fn(u64, usize) -> Result<Vec<u8>, String> + Send + Sync>,
    ) -> Result<Self, String> {
        let packet_hash = packet.pkt_hash().to_string();
        let headers = packet_headers(packet);

        let (kind, content_length, content_type) = if is_chunk_manifest(&headers) {
            let manifest = parse_chunk_manifest(&headers)
                .map_err(|e| format!("invalid chunk manifest: {e}"))?;
            (
                HpprResolvedMediaKind::ChunkManifest,
                manifest.total_length,
                manifest.content_type,
            )
        } else {
            (
                HpprResolvedMediaKind::Blob,
                packet.data().len() as u64,
                packet.header("Content-Type").map(str::to_string),
            )
        };

        let byte_source = Arc::new(ResolvedSourceByteSource::new(read_range, content_length));
        let asset = ResolvedMediaAsset::new(
            MediaAssetMetadata::new(content_length, content_type),
            byte_source,
        );
        Ok(Self {
            kind,
            endpoint,
            is_repo,
            packet_hash,
            asset,
        })
    }

    pub fn kind(&self) -> &HpprResolvedMediaKind {
        &self.kind
    }

    pub fn endpoint(&self) -> &str {
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

struct ResolvedSourceByteSource {
    read_range: Arc<dyn Fn(u64, usize) -> Result<Vec<u8>, String> + Send + Sync>,
    content_length: u64,
}

impl ResolvedSourceByteSource {
    fn new(
        read_range: Arc<dyn Fn(u64, usize) -> Result<Vec<u8>, String> + Send + Sync>,
        content_length: u64,
    ) -> Self {
        Self {
            read_range,
            content_length,
        }
    }
}

impl MediaByteSource for ResolvedSourceByteSource {
    fn read_range(&self, start: u64, len: usize) -> Result<Vec<u8>, String> {
        let Some((start, end)) = clamp_byte_range(self.content_length, start, len) else {
            return Ok(Vec::new());
        };
        let len = (end - start) as usize;
        (self.read_range)(start, len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hppr_packet::crypto::calculate_hash;
    use hppr_packet::writer::PacketWriter;
    use std::sync::Mutex;

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

    fn blob_hash(data: &[u8]) -> String {
        calculate_hash('B', [data])
    }

    #[test]
    fn blob_asset_reads_clamped_ranges() {
        let packet = build_blob_packet(b"abcdefghij", Some("video/mp4"));
        let asset = ResolvedHpprMediaAsset::from_packet(
            "127.0.0.1:4777".to_string(),
            true,
            &packet,
            Arc::new(|start, len| Ok(b"abcdefghij"[start as usize..start as usize + len].to_vec())),
        )
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

        let bytes = b"abcdefghijkl".to_vec();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_clone = calls.clone();
        let asset = ResolvedHpprMediaAsset::from_packet(
            "127.0.0.1:4777".to_string(),
            false,
            &packet,
            Arc::new(move |start, len| {
                calls_clone.lock().unwrap().push((start, len));
                Ok(bytes[start as usize..start as usize + len].to_vec())
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
        assert!(!calls.lock().unwrap().is_empty());
    }
}
