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

// ---------------------------------------------------------------------------
// Video ID
// ---------------------------------------------------------------------------

static NEXT_VIDEO_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_video_id() -> u64 {
    NEXT_VIDEO_ID.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// MediaSource (mirrors makepad_platform::VideoSource without the dep)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum MediaSource {
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
        source: MediaSource,
        /// Raw (namespace, index) image key for VideoTextureMap registration.
        image_key: (u32, u32),
        autoplay: bool,
        should_loop: bool,
    },
    /// Set up an audio-only player (no texture).
    PrepareAudio {
        video_id: u64,
        source: MediaSource,
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

/// Returns the canPlayType string for the given MIME type.
/// `""` = cannot play, `"maybe"` = might play, `"probably"` = can play.
///
/// Delegates to the Makepad platform backend via the stored callback so the
/// answer reflects what the current platform can actually decode.
pub fn can_play_type(mime: &str) -> &'static str {
    if let Some(f) = CAN_PLAY_TYPE_FN.lock().unwrap().as_ref() {
        f(mime)
    } else {
        // Fallback before platform callback is registered: conservative default.
        ""
    }
}

/// Platform callback type for canPlayType queries.
type CanPlayTypeFn = Box<dyn Fn(&str) -> &'static str + Send>;

static CAN_PLAY_TYPE_FN: Mutex<Option<CanPlayTypeFn>> = Mutex::new(None);

/// Register the platform's canPlayType implementation. Called once at startup.
pub fn set_can_play_type_fn(f: impl Fn(&str) -> &'static str + Send + 'static) {
    *CAN_PLAY_TYPE_FN.lock().unwrap() = Some(Box::new(f));
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
    /// Create a video controller and send PrepareVideo to the platform.
    pub fn new_video(
        source: MediaSource,
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
        Self {
            video_id,
            image_key,
            is_audio_only: false,
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
            paused: !autoplay,
            completed: false,
        }
    }

    /// Create an audio-only controller and send PrepareAudio to the platform.
    pub fn new_audio(source: MediaSource, autoplay: bool, should_loop: bool) -> Self {
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
        Self {
            video_id,
            image_key: (0, 0),
            is_audio_only: true,
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
            paused: !autoplay,
            completed: false,
        }
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
        }
    }
}

impl Drop for MediaController {
    fn drop(&mut self) {
        self.cleanup();
    }
}
