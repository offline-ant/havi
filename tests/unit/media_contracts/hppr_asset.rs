/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::Arc;

use hppr_packet::Packet;
use hppr_packet::crypto::calculate_hash;
use hppr_packet::writer::PacketWriter;
use media::{MediaAssetMetadata, ResolvedMediaAsset, clamp_byte_range};
use net::hppr_media::{HpprResolvedMediaKind, ResolvedHpprMediaAsset};

fn endpoint() -> String {
    "127.0.0.1:4777".to_string()
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

fn blob_hash(data: &[u8]) -> String {
    calculate_hash('B', [data])
}

fn slice_bytes(bytes: &[u8], start: u64, len: usize) -> Vec<u8> {
    let Some((start, end)) = clamp_byte_range(bytes.len() as u64, start, len) else {
        return Vec::new();
    };
    bytes[start as usize..end as usize].to_vec()
}

#[test]
fn media_contract_clamp_helper_is_explicit() {
    assert_eq!(clamp_byte_range(12, 3, 20), Some((3, 12)));
    assert_eq!(clamp_byte_range(12, 12, 1), None);

    let asset = ResolvedMediaAsset::new(
        MediaAssetMetadata::new(12, Some("video/mp4".into())),
        Arc::new(TestByteSource),
    );
    assert_eq!(asset.content_type(), Some("video/mp4"));
    assert_eq!(asset.read_range(2, 4).unwrap(), b"2345");
}

#[test]
fn hppr_blob_asset_reads_clamped_ranges() {
    let packet = build_blob_packet(b"abcdefghij", Some("video/mp4"));
    let full = b"abcdefghij".to_vec();
    let asset = ResolvedHpprMediaAsset::from_packet(
        endpoint(),
        true,
        &packet,
        Arc::new(move |start, len| Ok(slice_bytes(&full, start, len))),
    )
    .unwrap();

    assert_eq!(asset.kind(), &HpprResolvedMediaKind::Blob);
    assert!(asset.is_repo());
    assert_eq!(asset.asset().content_type(), Some("video/mp4"));
    assert_eq!(asset.packet_hash(), packet.pkt_hash());
    assert_eq!(asset.read_range(2, 4).unwrap(), b"cdef");
    assert_eq!(asset.read_range(8, 10).unwrap(), b"ij");
    assert!(asset.read_range(10, 4).unwrap().is_empty());
}

#[test]
fn hppr_chunk_manifest_asset_reads_random_ranges() {
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

    let full = b"abcdefghijkl".to_vec();
    let asset = ResolvedHpprMediaAsset::from_packet(
        endpoint(),
        false,
        &packet,
        Arc::new(move |start, len| Ok(slice_bytes(&full, start, len))),
    )
    .unwrap();

    assert_eq!(asset.kind(), &HpprResolvedMediaKind::ChunkManifest);
    assert!(!asset.is_repo());
    assert_eq!(asset.asset().content_type(), Some("video/mp4"));
    assert_eq!(asset.asset().content_length(), 12);
    assert_eq!(asset.read_range(1, 7).unwrap(), b"bcdefgh");
    assert_eq!(asset.read_range(6, 4).unwrap(), b"ghij");
    assert_eq!(asset.read_range(0, 12).unwrap(), b"abcdefghijkl");
}

#[test]
fn hppr_chunk_manifest_asset_uses_resolved_source_reads() {
    let inner_chunk = b"nested-content";
    let inner_hash = blob_hash(inner_chunk);
    let packet = build_manifest_packet(
        &[(0, inner_chunk.len() as u64, &inner_hash)],
        inner_chunk.len() as u64,
        Some("audio/mp4"),
    );

    let full = inner_chunk.to_vec();
    let asset = ResolvedHpprMediaAsset::from_packet(
        endpoint(),
        false,
        &packet,
        Arc::new(move |start, len| Ok(slice_bytes(&full, start, len))),
    )
    .unwrap();

    assert_eq!(asset.asset().content_type(), Some("audio/mp4"));
    assert_eq!(asset.read_range(0, inner_chunk.len()).unwrap(), inner_chunk);
    assert_eq!(asset.read_range(7, 7).unwrap(), b"content");
    assert!(!inner_hash.is_empty());
}

struct TestByteSource;

impl media::MediaByteSource for TestByteSource {
    fn read_range(&self, start: u64, len: usize) -> Result<Vec<u8>, String> {
        let full = b"0123456789ab";
        let Some((start, end)) = clamp_byte_range(full.len() as u64, start, len) else {
            return Ok(Vec::new());
        };
        Ok(full[start as usize..end as usize].to_vec())
    }
}
