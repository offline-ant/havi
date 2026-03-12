/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Browser-owned media asset contracts.
//!
//! The current Makepad playback/session APIs are synchronous. Session creation
//! and playback polling happen off the script thread, so the shared source
//! boundary for browser-owned transport is a blocking random-access byte source.
//! HPPR routing/auth/chunk resolution stays above this boundary in HAVI. The
//! media crate only receives resolved assets and byte reads.

use std::fmt;
use std::sync::Arc;

/// Metadata attached to a resolved media asset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaAssetMetadata {
    pub content_length: u64,
    pub content_type: Option<String>,
}

impl MediaAssetMetadata {
    pub fn new(content_length: u64, content_type: Option<String>) -> Self {
        Self {
            content_length,
            content_type,
        }
    }
}

/// Blocking byte source for resolved media assets.
///
/// Implementations must clamp reads to the asset length and return an empty
/// buffer when `start` is at or past end-of-stream.
pub trait MediaByteSource: Send + Sync {
    fn read_range(&self, start: u64, len: usize) -> Result<Vec<u8>, String>;
}

/// A browser-resolved media asset handed to the media playback layer.
#[derive(Clone)]
pub struct ResolvedMediaAsset {
    metadata: MediaAssetMetadata,
    byte_source: Arc<dyn MediaByteSource>,
}

impl ResolvedMediaAsset {
    pub fn new(metadata: MediaAssetMetadata, byte_source: Arc<dyn MediaByteSource>) -> Self {
        Self {
            metadata,
            byte_source,
        }
    }

    pub fn metadata(&self) -> &MediaAssetMetadata {
        &self.metadata
    }

    pub fn content_length(&self) -> u64 {
        self.metadata.content_length
    }

    pub fn content_type(&self) -> Option<&str> {
        self.metadata.content_type.as_deref()
    }

    pub fn byte_source(&self) -> Arc<dyn MediaByteSource> {
        Arc::clone(&self.byte_source)
    }

    pub fn read_range(&self, start: u64, len: usize) -> Result<Vec<u8>, String> {
        self.byte_source.read_range(start, len)
    }
}

impl fmt::Debug for ResolvedMediaAsset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedMediaAsset")
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

/// Clamp a requested byte range to the known asset length.
pub fn clamp_byte_range(content_length: u64, start: u64, len: usize) -> Option<(u64, u64)> {
    if len == 0 || start >= content_length {
        return None;
    }
    let end = start.saturating_add(len as u64).min(content_length);
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct RecordingSource {
        calls: Mutex<Vec<(u64, usize)>>,
    }

    impl RecordingSource {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl MediaByteSource for RecordingSource {
        fn read_range(&self, start: u64, len: usize) -> Result<Vec<u8>, String> {
            self.calls.lock().unwrap().push((start, len));
            Ok(vec![b'x'; len])
        }
    }

    #[test]
    fn clamp_range_truncates_to_end() {
        assert_eq!(clamp_byte_range(10, 3, 20), Some((3, 10)));
        assert_eq!(clamp_byte_range(10, 10, 1), None);
        assert_eq!(clamp_byte_range(10, 0, 0), None);
    }

    #[test]
    fn resolved_asset_delegates_reads() {
        let source = Arc::new(RecordingSource::new());
        let asset = ResolvedMediaAsset::new(
            MediaAssetMetadata::new(12, Some("video/mp4".into())),
            source.clone(),
        );

        assert_eq!(asset.content_length(), 12);
        assert_eq!(asset.content_type(), Some("video/mp4"));
        assert_eq!(asset.read_range(2, 4).unwrap(), b"xxxx");
        assert_eq!(source.calls.lock().unwrap().as_slice(), &[(2, 4)]);
    }
}
