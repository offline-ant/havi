use crossbeam_channel::Sender;
use euclid::Scale;
use libhavi::hppr::credentials::global_credential_store;
use makepad_widgets::event::VideoSource as PlatformVideoSource;
use makepad_widgets::makepad_platform::makepad_micro_serde::DeJson;
use makepad_widgets::makepad_platform::studio::StudioToApp;
use makepad_widgets::*;
use libhavi::media::controller::{
    self as media_controller, MediaEvent as ThreadMediaEvent, MediaOrigin as ThreadMediaOrigin,
    VideoOp,
};
use libhavi::media::ResolvedMediaAsset;
use libhavi::protocol_handler::ProtocolRegistry;
use libhavi::{DeviceIndependentPixel, DevicePixel, WebViewId};
use webrender_api::PipelineId;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Once;
use std::sync::mpsc;
use std::time::Instant;

mod actions;
mod camera;
mod capabilities;
mod clipboard;
mod context_menu;
mod delegate;
mod input_handling;
mod navigation;
mod overflow_menu;
mod pylon_menu;
mod runtime;
mod screenshot;
mod tabs;

use camera::CameraState;
use clipboard::ClipboardState;
use delegate::{HaviServoDelegate, HaviWebViewDelegate, MakepadEventLoopWaker, MakepadServoAction};
use navigation::NavCommand;
use pylon_menu::PylonStatus;
use tabs::{HOME_URL, TabInfo, title_from_url};

use crate::servo_web_view::ServoWebViewWidgetRefExt;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.HaviShellRoot
    use mod.widgets.HaviTabBar
    use mod.widgets.HaviToolbar
    use mod.widgets.HaviContextMenu
    use mod.widgets.HaviOverflowMenu
    use mod.widgets.HaviPylonMenu
    use mod.widgets.HaviSplash

    let app = startup() do #(App::script_component(vm)){
        ui: HaviShellRoot {}
    }
    app
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

fn settings_from_wire(mode: &str) -> Option<libhavi::hppr::watch::WatchSettings> {
    libhavi::hppr::watch::WatchSettings::from_wire(mode)
}

fn settings_to_wire(settings: libhavi::hppr::watch::WatchSettings) -> String {
    settings.to_wire()
}

fn watch_button_text(scope: libhavi::hppr::watch::WatchScope) -> &'static str {
    match scope {
        libhavi::hppr::watch::WatchScope::None => "Watch: Off",
        libhavi::hppr::watch::WatchScope::Page => "Watch: Page",
        libhavi::hppr::watch::WatchScope::App => "Watch: App",
    }
}

fn shadow_button_text(enabled: bool) -> &'static str {
    if enabled {
        "Shadow: On"
    } else {
        "Shadow: Off"
    }
}

fn dock_button_text(menu_at_bottom: bool) -> &'static str {
    if menu_at_bottom {
        "Move controls to top"
    } else {
        "Move controls to bottom"
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PylonMode {
    None,
    External,
    Embedded,
}

/// Result of background pylon + hpprd + credential bootstrap.
enum PylonInitResult {
    Ready {
        hpprd_port: u16,
        pylon_port: u16,
        pylon_events: std::sync::mpsc::Receiver<libhavi::hppr::pylon::PylonEvent>,
    },
    Failed {
        reason: String,
    },
}

#[derive(Clone, Debug, Default)]
pub(super) struct TabInspectorState {
    pub page_info: Option<libhavi::HpprPageInfo>,
    pub panel_open: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum StartupState {
    #[default]
    Booting,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug)]
struct PendingScreenshotCallback {
    request_id: u64,
}

fn pylon_mode_from_env() -> PylonMode {
    match std::env::var("HAVI_PYLON_MODE").ok().as_deref() {
        Some("none") => PylonMode::None,
        Some("external") => PylonMode::External,
        Some("embedded") if cfg!(feature = "embedded-services") => PylonMode::Embedded,
        Some("embedded") => PylonMode::External,
        _ if cfg!(feature = "embedded-services") => PylonMode::Embedded,
        _ => PylonMode::External,
    }
}

fn start_hpprd_with_runtime(
    pylon_client: &mut libhavi::hppr::pylon::PylonClient,
    runtime: Option<&str>,
) -> anyhow::Result<u16> {
    if let Some(port) = pylon_client.hpprd_port() {
        return Ok(port);
    }

    let mut args = serde_json::Map::new();
    if let Some(mode) = runtime {
        args.insert("runtime".to_string(), serde_json::json!(mode));
    }

    let start_error = pylon_client
        .command("start", Some("hpprd"), Some(&args))
        .err();

    let mut last_state: Option<String> = None;
    for _ in 0..20 {
        if let Some(port) = pylon_client.hpprd_port() {
            return Ok(port);
        }

        if let Ok(status) = pylon_client.command("status", None, None) {
            if let Some(hpprd) = status.get("hpprd") {
                let state = hpprd
                    .get("state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let pid = hpprd
                    .get("pid")
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "none".to_string());
                let port = hpprd
                    .get("port")
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "none".to_string());
                let addr = hpprd.get("addr").and_then(|v| v.as_str()).unwrap_or("none");
                last_state = Some(format!(
                    "state={}, pid={}, port={}, addr={}",
                    state, pid, port, addr
                ));
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(150));
    }

    let mut message = String::from(
        "hpprd did not become reachable after pylon startup request and 20 status polls (~3s).",
    );
    if let Some(e) = start_error {
        message.push_str(" Start request error: ");
        message.push_str(&e);
        message.push('.');
    }
    if let Some(state) = last_state {
        message.push_str(" Last observed hpprd status: ");
        message.push_str(&state);
        message.push('.');
    }

    Err(anyhow::anyhow!(message))
}

// ---------------------------------------------------------------------------
// ResourceReader
// ---------------------------------------------------------------------------

struct ResourceReader;

impl libhavi::resources::ResourceReaderMethods for ResourceReader {
    fn read(&self, res: libhavi::resources::Resource) -> Vec<u8> {
        libhavi::resources::embedded_default_bytes(res).to_owned()
    }

    fn sandbox_access_files_dirs(&self) -> Vec<std::path::PathBuf> {
        vec![]
    }

    fn sandbox_access_files(&self) -> Vec<std::path::PathBuf> {
        vec![]
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

app_main!(App);

impl App {
    pub(super) fn init_media_bridge(&mut self, cx: &mut Cx) {
        if self.video_op_rx.is_some() {
            return;
        }

        static INSTALL_MEDIA_PLUGIN: Once = Once::new();
        INSTALL_MEDIA_PLUGIN.call_once(makepad_media::install);

        let (tx, rx) = media_controller::create_video_op_channel();
        media_controller::set_video_op_sender(tx);
        media_controller::set_can_play_type_fn(makepad_widgets::makepad_platform::can_play_type);
        self.video_op_rx = Some(rx);
        cx.audio_output(0, move |info, output| {
            makepad_widgets::makepad_platform::mix_active_media_audio(info, output);
        });
        log!("[video] media bridge initialized");
    }

    fn attach_custom_playback_session(
        &mut self,
        cx: &mut Cx,
        video_id: u64,
        image_key: Option<(u32, u32)>,
        session_id: makepad_widgets::makepad_platform::MediaPlaybackSessionId,
        autoplay: bool,
        should_loop: bool,
    ) {
        let texture = Texture::new_with_format(cx, TextureFormat::VideoExternal);
        let texture_id = texture.texture_id();
        if let Some(image_key) = image_key {
            havi_render::video_texture_map::set_external_texture(image_key, texture);
            self.video_image_keys.insert(video_id, image_key);
        }
        self.video_logged_first_frame.remove(&video_id);
        self.video_texture_update_count.remove(&video_id);

        cx.prepare_video_playback(
            LiveId(video_id),
            PlatformVideoSource::PlaybackSession(session_id),
            makepad_widgets::makepad_platform::event::video_playback::CameraPreviewMode::Texture,
            0,
            texture_id,
            autoplay,
            should_loop,
        );
    }

    fn create_direct_playback_session(
        &self,
        mime: &str,
        asset: ResolvedMediaAsset,
    ) -> Result<makepad_widgets::makepad_platform::MediaPlaybackSessionId, String> {
        let content_length = asset.content_length();
        let byte_source = asset.byte_source();
        let reader: makepad_media::DirectByteSourceReader = std::sync::Arc::new(move |offset, len| {
            byte_source.read_range(offset, len)
        });
        makepad_media::register_direct_media_playback_session(
            reader,
            makepad_media::DirectMediaPlaybackConfig::new(content_length),
            mime,
        )
    }

    pub(super) fn drain_video_ops(&mut self, cx: &mut Cx) {
        let Some(rx) = self.video_op_rx.as_ref().cloned() else {
            return;
        };

        while let Ok(op) = rx.try_recv() {
            match op {
                VideoOp::PrepareVideo {
                    video_id,
                    webview_id,
                    source,
                    image_key,
                    autoplay,
                    should_loop,
                } => {
                    let texture = Texture::new_with_format(cx, TextureFormat::VideoExternal);
                    havi_render::video_texture_map::set_external_texture(image_key, texture.clone());
                    self.video_image_keys.insert(video_id, image_key);
                    self.video_webviews.insert(video_id, webview_id);
                    self.video_logged_first_frame.remove(&video_id);
                    self.video_texture_update_count.remove(&video_id);

                    log!(
                        "[video] prepare id={} key={:?} autoplay={} loop={}",
                        video_id,
                        image_key,
                        autoplay,
                        should_loop
                    );

                    cx.prepare_video_playback(
                        LiveId(video_id),
                        makepad_video_source(source),
                        makepad_widgets::makepad_platform::event::video_playback::CameraPreviewMode::Texture,
                        0,
                        texture.texture_id(),
                        autoplay,
                        should_loop,
                    );
                },
                VideoOp::PrepareAudio {
                    video_id,
                    source,
                    autoplay,
                    should_loop,
                } => {
                    log!(
                        "[video] prepare-audio id={} autoplay={} loop={}",
                        video_id,
                        autoplay,
                        should_loop
                    );
                    cx.prepare_audio_playback(
                        LiveId(video_id),
                        makepad_video_source(source),
                        autoplay,
                        should_loop,
                    );
                },
                VideoOp::PrepareDirectPlayback {
                    video_id,
                    webview_id,
                    asset,
                    mime,
                    image_key,
                    autoplay,
                    should_loop,
                } => match self.create_direct_playback_session(&mime, asset) {
                    Ok(session_id) => {
                        if image_key.is_some() {
                            self.video_webviews.insert(video_id, webview_id);
                        }
                        log!(
                            "[video] prepare-direct id={} mime={} image_key={:?} autoplay={} loop={}",
                            video_id,
                            mime,
                            image_key,
                            autoplay,
                            should_loop
                        );
                        self.attach_custom_playback_session(
                            cx,
                            video_id,
                            image_key,
                            session_id,
                            autoplay,
                            should_loop,
                        );
                    }
                    Err(e) => {
                        log!("[video] resolved prepare error id={}: {}", video_id, e);
                        media_controller::dispatch_media_event(
                            video_id,
                            ThreadMediaEvent::Error(e),
                        );
                    }
                },
                VideoOp::Play(video_id) => cx.begin_video_playback(LiveId(video_id)),
                VideoOp::Pause(video_id) => cx.pause_video_playback(LiveId(video_id)),
                VideoOp::Resume(video_id) => cx.resume_video_playback(LiveId(video_id)),
                VideoOp::Mute(video_id) => cx.mute_video_playback(LiveId(video_id)),
                VideoOp::Unmute(video_id) => cx.unmute_video_playback(LiveId(video_id)),
                VideoOp::Seek {
                    video_id,
                    position_ms,
                } => cx.seek_video_playback(LiveId(video_id), position_ms),
                VideoOp::SetVolume { video_id, volume } => {
                    cx.set_video_volume(LiveId(video_id), volume)
                },
                VideoOp::SetPlaybackRate { video_id, rate } => {
                    cx.set_video_playback_rate(LiveId(video_id), rate)
                },
                VideoOp::Cleanup(video_id) => {
                    if let Some(image_key) = self.video_image_keys.remove(&video_id) {
                        havi_render::video_texture_map::remove_video_binding(image_key);
                    }
                    self.video_webviews.remove(&video_id);
                    self.video_logged_first_frame.remove(&video_id);
                    self.video_texture_update_count.remove(&video_id);
                    self.mse_players.remove(&video_id);
                    cx.cleanup_video_playback_resources(LiveId(video_id));
                },

                // --- MSE operations ---

                VideoOp::PrepareMsePlayback {
                    video_id,
                    webview_id,
                    image_key,
                } => {
                    if image_key.is_some() {
                        self.video_webviews.insert(video_id, webview_id);
                    }
                    log!("[mse] prepare id={} key={:?}", video_id, image_key);
                    let handle = makepad_media::SharedMsePlaybackHandle::new();
                    let session_id = handle.register_session();
                    self.mse_players.insert(video_id, handle);
                    self.attach_custom_playback_session(
                        cx,
                        video_id,
                        image_key,
                        session_id,
                        false,
                        false,
                    );
                },
                VideoOp::MseAddSourceBuffer {
                    video_id,
                    input_id,
                    mime,
                } => {
                    match makepad_widgets::makepad_platform::media_plugin()
                        .ok_or_else(|| "no media plugin".to_string())
                        .and_then(|p| p.create_mse_playback_engine(&mime))
                    {
                        Ok(engine) => {
                            if let Some(player) = self.mse_players.get_mut(&video_id) {
                                if let Err(e) = player.add_input(input_id, engine) {
                                    media_controller::dispatch_media_event(
                                        video_id,
                                        ThreadMediaEvent::MseError {
                                            input_id,
                                            message: e,
                                        },
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            log!("[mse] error creating input id={} input={} : {}", video_id, input_id, e);
                            media_controller::dispatch_media_event(
                                video_id,
                                ThreadMediaEvent::MseError {
                                    input_id,
                                    message: e,
                                },
                            );
                        }
                    }
                },
                VideoOp::MseRemoveSourceBuffer { video_id, input_id } => {
                    if let Some(player) = self.mse_players.get_mut(&video_id) {
                        if let Err(e) = player.remove_input(input_id) {
                            media_controller::dispatch_media_event(
                                video_id,
                                ThreadMediaEvent::MseError {
                                    input_id,
                                    message: e,
                                },
                            );
                        }
                    }
                },
                VideoOp::MseAppendData {
                    video_id,
                    input_id,
                    data,
                } => {
                    if let Some(player) = self.mse_players.get_mut(&video_id) {
                        match player.append_data(input_id, &data) {
                            Ok(result) => {
                                if let Some(prepared) = result.input_prepared {
                                    log!(
                                        "[mse] init parsed id={} input={} {}x{} dur={}ms",
                                        video_id,
                                        input_id,
                                        prepared.width,
                                        prepared.height,
                                        prepared.duration_ms
                                    );
                                    media_controller::dispatch_media_event(
                                        video_id,
                                        ThreadMediaEvent::MseInitSegmentParsed {
                                            input_id,
                                            width: prepared.width,
                                            height: prepared.height,
                                            duration_ms: prepared.duration_ms,
                                            video_tracks: prepared.video_tracks.clone(),
                                            audio_tracks: prepared.audio_tracks.clone(),
                                        },
                                    );
                                }
                                if result.has_video_frames {
                                    if self.video_belongs_to_active_page(video_id) {
                                        self.note_active_page_visual_change();
                                    }
                                    self.request_spin_redraw(cx);
                                }
                                media_controller::dispatch_media_event(
                                    video_id,
                                    ThreadMediaEvent::MseAppendDone {
                                        input_id,
                                        input_buffered_ranges: result.input_buffered_ranges,
                                        buffered_ranges: result.buffered_ranges,
                                    },
                                );
                            }
                            Err(e) => {
                                log!("[mse] append error id={} input={}: {}", video_id, input_id, e);
                                media_controller::dispatch_media_event(
                                    video_id,
                                    ThreadMediaEvent::MseError {
                                        input_id,
                                        message: e,
                                    },
                                );
                            }
                        }
                    }
                },
                VideoOp::MseEndOfStream { video_id } => {
                    if let Some(player) = self.mse_players.get_mut(&video_id) {
                        if let Err(e) = player.end_of_stream() {
                            media_controller::dispatch_media_event(
                                video_id,
                                ThreadMediaEvent::Error(e),
                            );
                        }
                    }
                },
                VideoOp::MseRemove {
                    video_id,
                    input_id,
                    start,
                    end,
                } => {
                    if let Some(player) = self.mse_players.get_mut(&video_id) {
                        match player.remove(input_id, start, end) {
                            Ok(result) => {
                                media_controller::dispatch_media_event(
                                    video_id,
                                    ThreadMediaEvent::MseAppendDone {
                                        input_id,
                                        input_buffered_ranges: result.input_buffered_ranges,
                                        buffered_ranges: result.buffered_ranges,
                                    },
                                );
                            }
                            Err(e) => {
                                media_controller::dispatch_media_event(
                                    video_id,
                                    ThreadMediaEvent::MseError {
                                        input_id,
                                        message: e,
                                    },
                                );
                            }
                        }
                    }
                },
                VideoOp::MseSetAudioTrack { video_id, index, enabled } => {
                    if let Some(player) = self.mse_players.get_mut(&video_id) {
                        let _ = player.set_audio_track(index, enabled);
                    }
                },
                VideoOp::MseSetVideoTrack {
                    video_id,
                    index,
                    selected,
                } => {
                    if let Some(player) = self.mse_players.get_mut(&video_id) {
                        let _ = player.set_video_track(index, selected);
                    }
                },
            }
        }
    }

    pub(super) fn handle_video_event(&mut self, cx: &mut Cx, event: &Event) {
        match event {
            Event::VideoPlaybackPrepared(ev) => {
                log!(
                    "[video] prepared id={} {}x{} duration={}ms",
                    ev.video_id.0,
                    ev.video_width,
                    ev.video_height,
                    ev.duration
                );
                media_controller::dispatch_media_event(
                    ev.video_id.0,
                    ThreadMediaEvent::Prepared {
                        width: ev.video_width,
                        height: ev.video_height,
                        duration_ms: ev.duration,
                        is_seekable: ev.is_seekable,
                        video_tracks: ev.video_tracks.clone(),
                        audio_tracks: ev.audio_tracks.clone(),
                    },
                );
            },
            Event::VideoYuvTexturesReady(ev) => {
                let image_key = self
                    .video_image_keys
                    .get(&ev.video_id.0)
                    .copied()
                    .or_else(|| self.camera.image_key_for_video_id(ev.video_id.0));
                if let Some(image_key) = image_key {
                    havi_render::video_texture_map::set_yuv_planes(
                        image_key,
                        ev.tex_y.clone(),
                        ev.tex_u.clone(),
                        ev.tex_v.clone(),
                    );
                }
                if self.video_belongs_to_active_page(ev.video_id.0) {
                    self.note_active_page_visual_change();
                }
                self.request_spin_redraw(cx);
            },
            Event::VideoTextureUpdated(ev) => {
                let image_key = self
                    .video_image_keys
                    .get(&ev.video_id.0)
                    .copied()
                    .or_else(|| self.camera.image_key_for_video_id(ev.video_id.0));
                if let Some(image_key) = image_key {
                    havi_render::video_texture_map::set_yuv_metadata(image_key, ev.yuv);
                }

                let count = self
                    .video_texture_update_count
                    .entry(ev.video_id.0)
                    .and_modify(|n| *n += 1)
                    .or_insert(1);

                if self.video_logged_first_frame.insert(ev.video_id.0) {
                    log!(
                        "[video] first-frame id={} pos={}ms",
                        ev.video_id.0,
                        ev.current_position_ms
                    );
                }
                if *count <= 5 || *count % 30 == 0 {
                    log!(
                        "[video] frame id={} count={} pos={}ms",
                        ev.video_id.0,
                        count,
                        ev.current_position_ms
                    );
                }

                media_controller::dispatch_media_event(
                    ev.video_id.0,
                    ThreadMediaEvent::PositionChanged(ev.current_position_ms),
                );

                if self.video_belongs_to_active_page(ev.video_id.0) {
                    self.note_active_page_visual_change();
                }
                self.request_spin_redraw(cx);
            },
            Event::VideoPlaybackCompleted(ev) => {
                let frames = self
                    .video_texture_update_count
                    .get(&ev.video_id.0)
                    .copied()
                    .unwrap_or(0);
                log!("[video] completed id={} frames={}", ev.video_id.0, frames);
                media_controller::dispatch_media_event(
                    ev.video_id.0,
                    ThreadMediaEvent::PlaybackCompleted,
                );
            },
            Event::VideoDecodingError(ev) => {
                log!("[video] error id={} {}", ev.video_id.0, ev.error);
                media_controller::dispatch_media_event(
                    ev.video_id.0,
                    ThreadMediaEvent::Error(ev.error.clone()),
                );
            },
            Event::VideoSeekableRanges(ev) => {
                media_controller::dispatch_media_event(
                    ev.video_id.0,
                    ThreadMediaEvent::SeekableRanges(ev.ranges.clone()),
                );
            },
            Event::VideoBufferedRanges(ev) => {
                media_controller::dispatch_media_event(
                    ev.video_id.0,
                    ThreadMediaEvent::BufferedRanges(ev.ranges.clone()),
                );
            },
            Event::VideoPlaybackResourcesReleased(ev) => {
                if let Some(image_key) = self.video_image_keys.remove(&ev.video_id.0) {
                    havi_render::video_texture_map::remove_video_binding(image_key);
                }
                self.video_logged_first_frame.remove(&ev.video_id.0);
                self.video_texture_update_count.remove(&ev.video_id.0);
            },
            Event::VideoInputs(ev) => {
                self.camera.handle_video_inputs_event(ev);
            },
            _ => {},
        }
    }
}

fn makepad_video_source(source: ThreadMediaOrigin) -> PlatformVideoSource {
    match source {
        ThreadMediaOrigin::InMemory(data) => {
            // Fast path: avoid cloning the full in-memory payload when this Arc
            // has unique ownership at the hand-off boundary.
            let bytes = match std::sync::Arc::try_unwrap(data) {
                Ok(bytes) => bytes,
                Err(shared) => shared.as_ref().clone(),
            };
            PlatformVideoSource::InMemory(Rc::new(bytes))
        },
        ThreadMediaOrigin::Network(url) => PlatformVideoSource::Network(url),
        ThreadMediaOrigin::Filesystem(path) => PlatformVideoSource::Filesystem(path),
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[derive(Clone, Copy, Debug)]
pub(super) struct PendingClipboardMenu {
    anchor_abs: DVec2,
    baseline_revision: u64,
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,

    #[rust]
    clipboard_state: Option<Rc<ClipboardState>>,
    #[rust]
    servo: Option<libhavi::Servo>,
    #[rust]
    next_frame: NextFrame,
    #[rust]
    content_size: (usize, usize),
    #[rust]
    initialized: bool,
    #[rust]
    dpi_factor: f64,

    // --- Performance optimization state ---
    /// Whether the shell should keep spinning the next-frame loop because more
    /// work is pending.
    #[rust]
    needs_spin: bool,
    /// Number of frames since last loop activity. Used for idle detection to
    /// stop the frame loop when nothing is happening.
    #[rust]
    idle_frames: u32,
    /// Timestamp of the latest active-page visual change relevant to screenshot
    /// settling.
    #[rust]
    last_active_page_visual_change: Option<Instant>,

    // --- Touch gesture recognition state (used in handle_actions) ---
    /// Position (logical pixels) of the finger when it went down. Used to
    /// distinguish taps from scroll gestures.
    #[rust]
    finger_down_pos: Option<DVec2>,
    /// Whether the current finger gesture has been recognized as a scroll
    /// (finger moved beyond the tap threshold). Once true, Touch events are
    /// sent for the remainder of the gesture.
    #[rust]
    is_touch_scrolling: bool,
    /// Whether the current gesture originated from a mouse device. Mouse
    /// drags send MouseDown + MouseMove + MouseUp (for text selection)
    /// instead of Touch events (for scrolling).
    #[rust]
    is_mouse_gesture: bool,
    /// Whether a mouse drag is in progress (MouseDown sent to Servo).
    #[rust]
    is_mouse_dragging: bool,
    /// Whether the current gesture is a right-click. Right-clicks are sent
    /// to Servo immediately on finger-down; finger-up is suppressed.
    #[rust]
    is_right_click_gesture: bool,

    // --- Context menu state ---
    /// Active Servo context menu awaiting user selection. Presence means menu is open.
    #[rust]
    active_context_menu: Option<libhavi::ContextMenu>,
    #[rust]
    context_menu_pos: DVec2,
    /// Popup window handle for the context menu. None when menu is closed.
    #[rust]
    context_popup_window: Option<WindowHandle>,
    /// Draw pass for the context menu popup window.
    #[rust]
    context_popup_pass: Option<DrawPass>,
    /// Draw list for rendering context menu contents into the popup pass.
    #[rust]
    context_popup_draw_list: Option<DrawList2d>,
    /// Dynamic context-menu entries mirroring Servo's menu payload.
    #[rust]
    context_menu_entries: Vec<context_menu::ContextMenuEntry>,
    /// Cached ScriptObjectRef for context item template.
    #[rust]
    context_item_template_source: ScriptObjectRef,
    /// Cached ScriptObjectRef for context separator template.
    #[rust]
    context_separator_template_source: ScriptObjectRef,
    /// Latest known element flags for context-sensitive capabilities.
    #[rust]
    last_context_menu_flags: Option<libhavi::ContextMenuElementInformationFlags>,

    /// True while Servo reports an active IME/editable context.
    #[rust]
    ime_visible: bool,

    /// Latest advertised public via from pylon listener events.
    #[rust]
    shared_public_via: Option<String>,

    /// Pylon aggregate status for the status dot and dropdown menu.
    #[rust]
    pylon_status: PylonStatus,

    /// Whether the pylon dropdown menu is open.
    #[rust]
    pylon_menu_open: bool,

    /// Whether the advanced overflow menu is open.
    #[rust]
    overflow_menu_open: bool,

    /// Dedicated pylon TCP connection used for shell status refresh.
    /// The event stream connection is consumed by `subscribe()`.
    #[rust]
    pylon_command_client: Option<libhavi::hppr::pylon::PylonClient>,

    /// True when tab/toolbar chrome is docked to the bottom.
    #[rust]
    menu_at_bottom: bool,

    #[rust]
    tab_scroll_x: f64,

    // --- Tab state ---
    #[rust]
    tabs: Vec<TabInfo>,
    #[rust]
    active_tab_idx: usize,
    #[rust]
    active_root_pipeline_id: Option<PipelineId>,
    /// Cached ScriptObjectRef for the tab_template View. Extracted once from
    /// tab_bar children so the template widget is never kept as a hidden child
    /// (which caused ghost DrawQuad rendering artifacts on Linux/OpenGL).
    #[rust]
    tab_template_source: ScriptObjectRef,

    // --- Pylon event stream ---
    /// Receives pylon service events. The background reader thread keeps the
    /// TCP connection alive (preventing pylon idle shutdown).
    #[rust]
    pylon_events: Option<std::sync::mpsc::Receiver<libhavi::hppr::pylon::PylonEvent>>,

    /// Receiver for media-thread VideoOp commands (script thread -> makepad main thread).
    #[rust]
    video_op_rx: Option<crossbeam_channel::Receiver<VideoOp>>,

    /// Mapping from media video_id to image key for video texture registration.
    #[rust]
    video_image_keys: HashMap<u64, (u32, u32)>,

    /// Owning webview for each media video_id.
    #[rust]
    video_webviews: HashMap<u64, WebViewId>,

    /// Tracks whether a first frame has been observed for each video_id.
    #[rust]
    video_logged_first_frame: HashSet<u64>,

    /// Per-video number of VideoTextureUpdated events seen.
    #[rust]
    video_texture_update_count: HashMap<u64, u64>,

    /// Shared MSE session handles keyed by video_id.
    #[rust]
    mse_players: HashMap<u64, makepad_media::SharedMsePlaybackHandle>,

    /// True once default audio outputs have been selected for custom playback.
    #[rust]
    audio_outputs_initialized: bool,

    /// Camera subsystem state.
    #[rust]
    camera: CameraState,

    /// Last primary selection text sent to the platform, for change detection.
    #[cfg(target_os = "linux")]
    #[rust]
    last_primary_selection: String,

    /// Whether selection handles are currently shown (mobile).
    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[rust]
    selection_handles_visible: bool,

    /// Pending clipboard-action menu request waiting for fresh selection snapshot.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[rust]
    pending_clipboard_menu: Option<PendingClipboardMenu>,

    /// Canonical startup URL selected once during init.
    #[rust]
    start_url: String,

    /// True once startup open delivery is allowed.
    #[rust]
    start_navigation_done: bool,


    /// Startup state machine.
    #[rust]
    startup_state: StartupState,

    /// Receives the pylon init result from the background thread.
    #[rust]
    pylon_init_rx: Option<std::sync::mpsc::Receiver<PylonInitResult>>,

    /// Shared HPPR watch connection pool.
    /// Field order matters: this is dropped before `havi_runtime` during App teardown.
    #[rust]
    watch_pool: Option<libhavi::hppr::watch::WatchPool>,

    /// Endpoint used when initializing watch pool lazily.
    #[rust]
    watch_fallback_endpoint: String,

    /// Dedicated runtime for UI-owned async tasks (watch connections).
    /// Declared after `watch_pool` so watch tasks are aborted before runtime teardown.
    #[rust]
    havi_runtime: Option<tokio::runtime::Runtime>,

    /// Timer for splash screen timeout (3 seconds max during pylon boot).
    #[rust]
    splash_timeout: Timer,

    /// Timer driving screenshot state progression independently of render activity.
    #[rust]
    screenshot_poll: Timer,

    #[rust]
    pending_screenshot_callbacks: HashMap<u64, PendingScreenshotCallback>,

    #[rust]
    screenshot_mode: Option<screenshot::ScreenshotMode>,
}

/// Maximum number of idle frames before stopping the frame loop.
/// When the frame loop stops, Servo's `wake()` call will restart it.
const MAX_IDLE_FRAMES: u32 = 10;

/// Distance threshold (in logical pixels) to distinguish taps from scrolls.
/// If the finger moves more than this distance from the initial touch point,
/// the gesture is treated as a scroll; otherwise it's a tap (click).
const TAP_DISTANCE_THRESHOLD: f64 = 5.0;

impl App {
    pub(super) fn drain_pending_screenshot_requests(&mut self, cx: &mut Cx) {
        let Some(servo) = &self.servo else {
            return;
        };

        for request in servo.paint_screenshot_bridge().drain_requests() {
            if !self
                .tabs
                .get(self.active_tab_idx)
                .map(|tab| tab.webview_id == request.webview_id)
                .unwrap_or(false)
            {
                continue;
            }

            let source = match self
                .ui
                .servo_web_view(cx, ids!(web_view))
                .prepare_capture_source(cx)
            {
                Ok(source) => source,
                Err(err) => {
                    eprintln!(
                        "[havi][screenshot] failed to prepare browser capture source for webview {:?}: {}",
                        request.webview_id,
                        err
                    );
                    continue;
                }
            };
            let capture_request_id = cx.request_capture(source);
            self.pending_screenshot_callbacks.insert(
                capture_request_id,
                PendingScreenshotCallback {
                    request_id: request.request_id,
                },
            );
            self.next_frame = cx.new_next_frame();
            cx.redraw_all();
        }
    }

    pub(super) fn request_spin(&mut self, cx: &mut Cx) {
        self.needs_spin = true;
        self.idle_frames = 0;
        self.next_frame = cx.new_next_frame();
    }

    pub(super) fn request_spin_redraw(&mut self, cx: &mut Cx) {
        self.request_spin(cx);
        cx.redraw_all();
    }

    pub(super) fn note_active_page_visual_change(&mut self) {
        self.last_active_page_visual_change = Some(Instant::now());
    }

    pub(super) fn request_active_page_redraw(&mut self, cx: &mut Cx) {
        self.note_active_page_visual_change();
        self.request_spin_redraw(cx);
    }

    pub(super) fn active_webview_id(&self) -> Option<WebViewId> {
        self.tabs.get(self.active_tab_idx).map(|tab| tab.webview_id)
    }

    pub(super) fn active_inspector_state(&self) -> Option<&TabInspectorState> {
        self.tabs.get(self.active_tab_idx).map(|tab| &tab.inspector)
    }

    pub(super) fn video_belongs_to_active_page(&self, video_id: u64) -> bool {
        self.video_webviews
            .get(&video_id)
            .zip(self.active_webview_id())
            .is_some_and(|(video_webview_id, active_webview_id)| {
                *video_webview_id == active_webview_id
            })
    }

    pub(super) fn current_render_fragments(&self) -> libhavi::layout::SharedLayoutFragmentTree {
        let tab = &self.tabs[self.active_tab_idx];
        libhavi::layout::shared_layout_fragment_tree_for(tab.webview_id)
    }

    pub(super) fn current_shell_scroll_state(&self) -> libhavi::layout::SharedScrollState {
        let tab = &self.tabs[self.active_tab_idx];
        libhavi::layout::shared_scroll_state_for(tab.webview_id)
    }

    pub(super) fn attach_active_browser_state(&self, cx: &mut Cx) {
        let Some(tab) = self.tabs.get(self.active_tab_idx) else {
            return;
        };
        let shared = self.current_render_fragments();
        let scroll = self.current_shell_scroll_state();
        let selection = libhavi::layout::shared_document_selection_for(tab.webview_id);
        let image_sources = self.servo.as_ref().unwrap().image_source_store();
        self.ui.servo_web_view(cx, ids!(web_view)).set_shared_browser_state(
            cx,
            tab.webview_id,
            tab.root_pipeline_id,
            shared,
            scroll,
            selection,
            image_sources,
        );
        self.ui.servo_web_view(cx, ids!(web_view)).redraw(cx);
    }

    pub(super) fn focus_active_webview(&self, cx: &mut Cx) {
        let area = self.ui.servo_web_view(cx, ids!(web_view)).area();
        if !area.is_empty() && !cx.has_key_focus(area) {
            cx.set_key_focus(area);
        }
    }

    pub(super) fn sanitize_url_bar_text(text: &str) -> String {
        text.chars()
            .filter(|c| !matches!(c, '\r' | '\n' | '\t'))
            .collect()
    }

    pub(super) fn set_url_input_sanitized(&self, cx: &mut Cx, text: &str) {
        let sanitized = Self::sanitize_url_bar_text(text);
        self.ui.text_input(cx, ids!(url_input)).set_text(cx, &sanitized);
    }

    pub(super) fn read_url_input_sanitized(&self, cx: &mut Cx) -> String {
        let text = self.ui.text_input(cx, ids!(url_input)).text();
        Self::sanitize_url_bar_text(&text)
    }

    pub(super) fn is_primary_new_tab_shortcut(ke: &KeyEvent) -> bool {
        ke.key_code == KeyCode::KeyT
            && (ke.modifiers.control || ke.modifiers.logo)
            && !ke.modifiers.shift
            && !ke.is_repeat
    }
}
