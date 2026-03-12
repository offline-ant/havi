/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! MediaController: direct Makepad video/audio playback from HTMLMediaElement.
//!
//! Replaces the servo-media `Player` trait. HTMLMediaElement creates a
//! `MediaController` which sends `VideoOp`s through a crossbeam channel to
//! the Makepad event loop in havishell. Events flow back the other way via
//! per-controller crossbeam channels bridged to the script task source.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use log::{info, warn};

use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::asset::ResolvedMediaAsset;

// ---------------------------------------------------------------------------
// Video ID
// ---------------------------------------------------------------------------

static NEXT_VIDEO_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_video_id() -> u64 {
    NEXT_VIDEO_ID.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// MediaOrigin (legacy direct-source boundary for native delegated playback)
// ---------------------------------------------------------------------------
//
// Browser-owned transport should move through `asset::ResolvedMediaAsset` +
// `asset::MediaByteSource`. This enum remains for the current delegated native
// path and existing platform URL/file session setup.

#[derive(Clone, Debug)]
pub enum MediaOrigin {
    /// In-memory bytes (e.g. decoded from a data: URL or blob).
    InMemory(std::sync::Arc<Vec<u8>>),
    /// HTTP/HPPR URL.
    Network(String),
    /// Local filesystem path.
    Filesystem(String),
}

// ---------------------------------------------------------------------------
// VideoOp — script thread → Makepad event loop
// ---------------------------------------------------------------------------

pub enum VideoOp {
    /// Set up a video player with texture output.
    PrepareVideo {
        video_id: u64,
        source: MediaOrigin,
        /// Raw (namespace, index) image key for VideoTextureMap registration.
        image_key: (u32, u32),
        autoplay: bool,
        should_loop: bool,
    },
    /// Set up an audio-only player (no texture).
    PrepareAudio {
        video_id: u64,
        source: MediaOrigin,
        autoplay: bool,
        should_loop: bool,
    },
    /// Set up a browser-owned resolved MP4/fMP4 asset on the shared custom
    /// playback-session path.
    PrepareResolvedPlayback {
        video_id: u64,
        asset: ResolvedMediaAsset,
        mime: String,
        /// None for audio-only playback.
        image_key: Option<(u32, u32)>,
        autoplay: bool,
        should_loop: bool,
    },
    Play(u64),
    Pause(u64),
    Resume(u64),
    Mute(u64),
    Unmute(u64),
    Seek {
        video_id: u64,
        position_ms: u64,
    },
    SetVolume {
        video_id: u64,
        volume: f64,
    },
    SetPlaybackRate {
        video_id: u64,
        rate: f64,
    },
    Cleanup(u64),

    // --- MSE operations ---

    /// Set up an MSE-backed custom playback session (no source URL; data is
    /// pushed via `MseAppendData`).
    PrepareMsePlayback {
        video_id: u64,
        /// MIME type with codecs parameter, e.g. `video/mp4; codecs="av01.0.04M.08"`.
        mime: String,
        /// None for audio-only playback.
        image_key: Option<(u32, u32)>,
    },
    /// Push fMP4 data (init segment or media segment) to an MSE player.
    MseAppendData {
        video_id: u64,
        data: Vec<u8>,
    },
    /// Signal end of stream for an MSE player.
    MseEndOfStream {
        video_id: u64,
    },
    /// Remove buffered data in a time range (seconds).
    MseRemove {
        video_id: u64,
        start: f64,
        end: f64,
    },
}

// ---------------------------------------------------------------------------
// MediaEvent — Makepad event loop → script thread
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum MediaEvent {
    /// Metadata is ready; playback can begin.
    Prepared {
        width: u32,
        height: u32,
        duration_ms: u128,
        is_seekable: bool,
        video_tracks: Vec<String>,
        audio_tracks: Vec<String>,
    },
    /// Current position update (milliseconds).
    PositionChanged(u128),
    /// Playback reached end of stream.
    PlaybackCompleted,
    /// Decoding or platform error.
    Error(String),
    /// Seekable time ranges (seconds).
    SeekableRanges(Vec<(f64, f64)>),
    /// Buffered (downloaded) time ranges (seconds).
    BufferedRanges(Vec<(f64, f64)>),

    // --- MSE events ---

    /// MSE append operation completed; source buffer can accept more data.
    MseAppendDone {
        buffered_ranges: Vec<(f64, f64)>,
    },
    /// MSE init segment parsed; video metadata available.
    MseInitSegmentParsed {
        width: u32,
        height: u32,
        duration_ms: u128,
    },
    /// MSE append or decode error.
    MseError(String),
}

// ---------------------------------------------------------------------------
// Global VideoOp channel
// ---------------------------------------------------------------------------

/// Sender half of the VideoOp channel. Set by havishell at startup.
static VIDEO_OP_SENDER: Mutex<Option<Sender<VideoOp>>> = Mutex::new(None);

/// Emit the missing-bridge warning only once.
static VIDEO_OP_SENDER_WARNED: AtomicBool = AtomicBool::new(false);

/// Set the global VideoOp sender. Called once by havishell during init.
pub fn set_video_op_sender(sender: Sender<VideoOp>) {
    *VIDEO_OP_SENDER.lock().unwrap() = Some(sender);
}

/// Create the VideoOp channel. Returns (sender, receiver). The sender is
/// stored globally via set_video_op_sender(); the receiver is stored in App.
pub fn create_video_op_channel() -> (Sender<VideoOp>, Receiver<VideoOp>) {
    unbounded()
}

fn send_op(op: VideoOp) {
    if let Some(sender) = VIDEO_OP_SENDER.lock().unwrap().as_ref() {
        let _ = sender.send(op);
    } else if !VIDEO_OP_SENDER_WARNED.swap(true, Ordering::Relaxed) {
        warn!("media: video op dropped because havishell media bridge is not initialized");
    }
}

// ---------------------------------------------------------------------------
// Global MediaEvent registry
// ---------------------------------------------------------------------------

/// Per-video event senders. Each MediaController registers on creation and
/// deregisters on cleanup. Havishell looks up by video_id to forward events.
static MEDIA_EVENT_SENDERS: LazyLock<Mutex<HashMap<u64, Sender<MediaEvent>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register an event sender for a video_id.
pub fn register_event_sender(video_id: u64, sender: Sender<MediaEvent>) {
    MEDIA_EVENT_SENDERS.lock().unwrap().insert(video_id, sender);
}

/// Remove the event sender for a video_id.
pub fn deregister_event_sender(video_id: u64) {
    MEDIA_EVENT_SENDERS.lock().unwrap().remove(&video_id);
}

pub fn append_mse_data(video_id: u64, data: Vec<u8>) {
    send_op(VideoOp::MseAppendData { video_id, data });
}

pub fn end_mse_stream(video_id: u64) {
    send_op(VideoOp::MseEndOfStream { video_id });
}

pub fn remove_mse_data(video_id: u64, start: f64, end: f64) {
    send_op(VideoOp::MseRemove {
        video_id,
        start,
        end,
    });
}

/// Returns the canPlayType string for the given MIME type.
/// `""` = cannot play, `"maybe"` = might play, `"probably"` = can play.
///
/// HAVI video policy: AV1 and H.264 in MP4. All other video containers and
/// codecs return `""`. Audio types delegate to the platform backend.
pub fn can_play_type(mime: &str) -> &'static str {
    let (base, codecs) = parse_mime_codecs(mime);

    if base.starts_with("video/") {
        return can_play_video_type(base, codecs);
    }

    // Audio types: delegate to platform backend.
    if let Some(f) = CAN_PLAY_TYPE_FN.lock().unwrap().as_ref() {
        f(mime)
    } else {
        ""
    }
}

/// HAVI video codec policy.
///
/// Supported video codecs: AV1 (`av01`) and H.264 (`avc1`, `avc3`).
///
/// - `video/mp4` without codecs → `"maybe"`
/// - `video/mp4` with supported video codecs → `"probably"`
/// - `video/mp4` with unsupported video codecs → `""`
/// - All other video containers → `""`
fn can_play_video_type(base: &str, codecs: Option<&str>) -> &'static str {
    if base != "video/mp4" && base != "video/x-m4v" {
        return "";
    }

    let Some(codecs) = codecs else {
        return "maybe";
    };

    // Parse comma-separated codec list. Every video codec must be av01 or avc1/avc3.
    // Known audio codecs (opus, mp4a, flac) are acceptable companions.
    let mut has_video_codec = false;
    for codec in codecs.split(',') {
        let c = codec.trim();
        if c.is_empty() {
            continue;
        }
        if c.starts_with("av01") || c.starts_with("avc1") || c.starts_with("avc3") {
            has_video_codec = true;
        } else if c.starts_with("mp4a")
            || c.starts_with("opus")
            || c.starts_with("flac")
            || c.starts_with("Opus")
        {
            // Acceptable audio companion codec.
        } else {
            // Unsupported video codec (hev1, vp09, etc.)
            return "";
        }
    }

    if has_video_codec { "probably" } else { "maybe" }
}

/// Split a MIME string into base type and optional codecs parameter value.
/// E.g. `video/mp4; codecs="av01.0.04M.08"` → `("video/mp4", Some("av01.0.04M.08"))`.
fn parse_mime_codecs(mime: &str) -> (&str, Option<&str>) {
    let base = mime.split(';').next().unwrap_or("").trim();
    let codecs = mime
        .split(';')
        .skip(1)
        .find_map(|param| {
            let param = param.trim();
            let param = param.strip_prefix("codecs=")?;
            // Strip optional quotes.
            let param = param.trim_matches('"').trim_matches('\'');
            Some(param)
        });
    (base, codecs)
}

/// Platform callback type for canPlayType queries (used for audio types).
type CanPlayTypeFn = Box<dyn Fn(&str) -> &'static str + Send>;

static CAN_PLAY_TYPE_FN: Mutex<Option<CanPlayTypeFn>> = Mutex::new(None);

/// Register the platform's canPlayType implementation. Called once at startup.
pub fn set_can_play_type_fn(f: impl Fn(&str) -> &'static str + Send + 'static) {
    *CAN_PLAY_TYPE_FN.lock().unwrap() = Some(Box::new(f));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn av1_mp4_probably() {
        assert_eq!(can_play_video_type("video/mp4", Some("av01.0.04M.08")), "probably");
    }

    #[test]
    fn av1_mp4_with_opus() {
        assert_eq!(can_play_video_type("video/mp4", Some("av01.0.04M.08, opus")), "probably");
    }

    #[test]
    fn av1_mp4_with_mp4a() {
        assert_eq!(can_play_video_type("video/mp4", Some("av01.0.04M.08, mp4a.40.2")), "probably");
    }

    #[test]
    fn bare_mp4_maybe() {
        assert_eq!(can_play_video_type("video/mp4", None), "maybe");
    }

    #[test]
    fn h264_mp4_probably() {
        assert_eq!(can_play_video_type("video/mp4", Some("avc1.42E01E")), "probably");
    }

    #[test]
    fn h264_avc3_mp4_probably() {
        assert_eq!(can_play_video_type("video/mp4", Some("avc3.42E01E")), "probably");
    }

    #[test]
    fn h264_with_aac_probably() {
        assert_eq!(can_play_video_type("video/mp4", Some("avc1.42E01E, mp4a.40.2")), "probably");
    }

    #[test]
    fn av1_and_h264_mixed() {
        assert_eq!(can_play_video_type("video/mp4", Some("av01.0.04M.08, avc1.42E01E")), "probably");
    }

    #[test]
    fn h265_mp4_rejected() {
        assert_eq!(can_play_video_type("video/mp4", Some("hev1.1.6.L93.B0")), "");
    }

    #[test]
    fn vp9_mp4_rejected() {
        assert_eq!(can_play_video_type("video/mp4", Some("vp09.00.10.08")), "");
    }

    #[test]
    fn webm_rejected() {
        assert_eq!(can_play_video_type("video/webm", None), "");
        assert_eq!(can_play_video_type("video/webm", Some("vp8")), "");
        assert_eq!(can_play_video_type("video/webm", Some("vp9")), "");
        assert_eq!(can_play_video_type("video/webm", Some("av01.0.04M.08")), "");
    }

    #[test]
    fn ogg_rejected() {
        assert_eq!(can_play_video_type("video/ogg", None), "");
    }

    #[test]
    fn matroska_rejected() {
        assert_eq!(can_play_video_type("video/x-matroska", None), "");
    }

    #[test]
    fn parse_codecs_basic() {
        let (base, codecs) = parse_mime_codecs("video/mp4; codecs=\"av01.0.04M.08\"");
        assert_eq!(base, "video/mp4");
        assert_eq!(codecs, Some("av01.0.04M.08"));
    }

    #[test]
    fn parse_codecs_unquoted() {
        let (base, codecs) = parse_mime_codecs("video/mp4; codecs=av01.0.04M.08");
        assert_eq!(base, "video/mp4");
        assert_eq!(codecs, Some("av01.0.04M.08"));
    }

    #[test]
    fn parse_codecs_absent() {
        let (base, codecs) = parse_mime_codecs("video/webm");
        assert_eq!(base, "video/webm");
        assert_eq!(codecs, None);
    }
}

/// Send a MediaEvent to the controller registered for video_id.
pub fn dispatch_media_event(video_id: u64, event: MediaEvent) {
    if let Some(sender) = MEDIA_EVENT_SENDERS.lock().unwrap().get(&video_id) {
        let _ = sender.send(event);
    }
}

// ---------------------------------------------------------------------------
// MediaController
// ---------------------------------------------------------------------------

/// State maintained locally by the MediaController on the script thread.
pub struct MediaController {
    pub video_id: u64,
    /// Raw (namespace, index) for VideoTextureMap lookup. Zero for audio-only.
    pub image_key: (u32, u32),
    pub is_audio_only: bool,

    // Metadata filled in when MediaEvent::Prepared arrives.
    pub prepared: bool,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u128,
    pub is_seekable: bool,
    pub video_tracks: Vec<String>,
    pub audio_tracks: Vec<String>,

    // Runtime state.
    pub position_ms: u128,
    pub seekable_ranges: Vec<(f64, f64)>,
    pub buffered_ranges: Vec<(f64, f64)>,
    pub paused: bool,
    pub completed: bool,
}

impl MediaController {
    fn new_common(video_id: u64, image_key: (u32, u32), is_audio_only: bool, paused: bool) -> Self {
        Self {
            video_id,
            image_key,
            is_audio_only,
            prepared: false,
            width: 0,
            height: 0,
            duration_ms: 0,
            is_seekable: false,
            video_tracks: vec![],
            audio_tracks: vec![],
            position_ms: 0,
            seekable_ranges: vec![],
            buffered_ranges: vec![],
            paused,
            completed: false,
        }
    }

    /// Create a video controller and send PrepareVideo to the platform.
    pub fn new_video(
        source: MediaOrigin,
        image_key: (u32, u32),
        autoplay: bool,
        should_loop: bool,
    ) -> Self {
        let video_id = next_video_id();
        info!(
            "media: queue PrepareVideo id={} image_key={:?} autoplay={} loop={}",
            video_id, image_key, autoplay, should_loop
        );
        send_op(VideoOp::PrepareVideo {
            video_id,
            source,
            image_key,
            autoplay,
            should_loop,
        });
        Self::new_common(video_id, image_key, false, !autoplay)
    }

    /// Create an audio-only controller and send PrepareAudio to the platform.
    pub fn new_audio(source: MediaOrigin, autoplay: bool, should_loop: bool) -> Self {
        let video_id = next_video_id();
        info!(
            "media: queue PrepareAudio id={} autoplay={} loop={}",
            video_id, autoplay, should_loop
        );
        send_op(VideoOp::PrepareAudio {
            video_id,
            source,
            autoplay,
            should_loop,
        });
        Self::new_common(video_id, (0, 0), true, !autoplay)
    }

    /// Create a resolved custom-playback controller from browser-owned MP4/fMP4
    /// source bytes. This stays on the shared playback-session path used by MSE.
    pub fn new_resolved_playback(
        asset: ResolvedMediaAsset,
        mime: String,
        image_key: Option<(u32, u32)>,
        autoplay: bool,
        should_loop: bool,
    ) -> Self {
        let video_id = next_video_id();
        info!(
            "media: queue PrepareResolvedPlayback id={} mime={} image_key={:?} autoplay={} loop={}",
            video_id, mime, image_key, autoplay, should_loop
        );
        send_op(VideoOp::PrepareResolvedPlayback {
            video_id,
            asset,
            mime,
            image_key,
            autoplay,
            should_loop,
        });
        Self::new_common(video_id, image_key.unwrap_or((0, 0)), image_key.is_none(), !autoplay)
    }

    /// Create an MSE-backed controller on the shared custom playback path.
    pub fn new_mse_playback(
        mime: String,
        image_key: Option<(u32, u32)>,
        autoplay: bool,
        should_loop: bool,
    ) -> Self {
        let video_id = next_video_id();
        info!(
            "media: queue PrepareMsePlayback id={} mime={} image_key={:?} autoplay={} loop={}",
            video_id, mime, image_key, autoplay, should_loop
        );
        send_op(VideoOp::PrepareMsePlayback {
            video_id,
            mime,
            image_key,
        });
        Self::new_common(video_id, image_key.unwrap_or((0, 0)), image_key.is_none(), !autoplay)
    }

    pub fn play(&mut self) {
        self.paused = false;
        send_op(VideoOp::Play(self.video_id));
    }

    pub fn pause(&mut self) {
        self.paused = true;
        send_op(VideoOp::Pause(self.video_id));
    }

    pub fn resume(&mut self) {
        self.paused = false;
        send_op(VideoOp::Resume(self.video_id));
    }

    pub fn mute(&self) {
        send_op(VideoOp::Mute(self.video_id));
    }

    pub fn unmute(&self) {
        send_op(VideoOp::Unmute(self.video_id));
    }

    pub fn seek(&self, position_ms: u64) {
        send_op(VideoOp::Seek {
            video_id: self.video_id,
            position_ms,
        });
    }

    pub fn set_volume(&self, volume: f64) {
        send_op(VideoOp::SetVolume {
            video_id: self.video_id,
            volume,
        });
    }

    pub fn set_playback_rate(&self, rate: f64) {
        send_op(VideoOp::SetPlaybackRate {
            video_id: self.video_id,
            rate,
        });
    }

    pub fn cleanup(&self) {
        send_op(VideoOp::Cleanup(self.video_id));
        deregister_event_sender(self.video_id);
    }

    /// Apply a MediaEvent, updating internal state. Called on the script thread
    /// via the bridge task.
    pub fn apply_event(&mut self, event: &MediaEvent) {
        match event {
            MediaEvent::Prepared {
                width,
                height,
                duration_ms,
                is_seekable,
                video_tracks,
                audio_tracks,
            } => {
                self.prepared = true;
                self.width = *width;
                self.height = *height;
                self.duration_ms = *duration_ms;
                self.is_seekable = *is_seekable;
                self.video_tracks = video_tracks.clone();
                self.audio_tracks = audio_tracks.clone();
            },
            MediaEvent::PositionChanged(pos) => {
                self.position_ms = *pos;
            },
            MediaEvent::PlaybackCompleted => {
                self.completed = true;
                self.paused = true;
            },
            MediaEvent::Error(_) => {},
            MediaEvent::SeekableRanges(ranges) => {
                self.seekable_ranges = ranges.clone();
            },
            MediaEvent::BufferedRanges(ranges) => {
                self.buffered_ranges = ranges.clone();
            },
            MediaEvent::MseAppendDone { buffered_ranges } => {
                self.buffered_ranges = buffered_ranges.clone();
            },
            MediaEvent::MseInitSegmentParsed { width, height, duration_ms } => {
                self.prepared = true;
                self.width = *width;
                self.height = *height;
                self.duration_ms = *duration_ms;
            },
            MediaEvent::MseError(_) => {},
        }
    }
}

impl Drop for MediaController {
    fn drop(&mut self) {
        self.cleanup();
    }
}
