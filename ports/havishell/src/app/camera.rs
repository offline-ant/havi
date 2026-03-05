//! Camera subsystem: translates CameraRequest embedder messages into
//! Makepad CxMediaApi / video playback calls.
//!
//! Camera is treated as "a video whose source is a device." Frames go
//! directly to a Makepad Texture via `prepare_video_playback` with
//! `VideoSource::Camera`, same as decoded video. Script only receives
//! an image_key and dimensions — no pixel data crosses this boundary.

use super::*;
use makepad_widgets::makepad_platform::video::{VideoFormatId, VideoInputId};
use servo::{CameraRequest, CameraStreamInfo};

/// Per-stream bookkeeping.
struct ActiveStream {
    /// Makepad video_id used with prepare_video_playback / cleanup.
    video_id: u64,
    /// Raw (namespace, index) image key in VideoTextureMap.
    image_key: (u32, u32),
}

/// Camera subsystem state, owned by App.
#[derive(Default)]
pub(super) struct CameraState {
    next_stream_id: u64,
    next_video_id: u64,
    /// stream_id → active stream state.
    streams: HashMap<u64, ActiveStream>,
    /// Cached device list from last VideoInputs event.
    devices: Option<Vec<servo::CameraDeviceInfo>>,
}

impl CameraState {
    pub(super) fn handle_video_inputs_event(
        &mut self,
        ev: &VideoInputsEvent,
    ) {
        let mut devices = Vec::new();
        for desc in &ev.descs {
            let mut formats = Vec::new();
            for fmt in &desc.formats {
                formats.push(servo::CameraFormat {
                    width: fmt.width as u32,
                    height: fmt.height as u32,
                    frame_rate: fmt.frame_rate.unwrap_or(30.0),
                });
            }
            devices.push(servo::CameraDeviceInfo {
                device_id: desc.input_id.0 .0.to_string(),
                label: desc.name.clone(),
                formats,
            });
        }
        self.devices = Some(devices);
    }

    pub(super) fn handle_request(&mut self, cx: &mut Cx, request: CameraRequest) {
        match request {
            CameraRequest::EnumerateDevices(sender) => {
                let devices = self.devices.clone().unwrap_or_default();
                let _ = sender.send(devices);
            },
            CameraRequest::Open {
                device_id: _,
                width: _,
                height: _,
                frame_rate: _,
                response,
            } => {
                let stream_id = self.next_stream_id;
                self.next_stream_id += 1;

                let video_id = self.next_video_id;
                self.next_video_id += 1;

                // Allocate a texture and register it in VideoTextureMap,
                // same as video playback does in drain_video_ops.
                let texture = Texture::new_with_format(cx, TextureFormat::VideoExternal);
                let image_key = (stream_id as u32, video_id as u32);
                havi_render::video_texture_map::register_video_texture(
                    image_key,
                    texture.clone(),
                );

                // Use the first available device with default format.
                // TODO: match requested device_id/width/height/frame_rate
                // against cached device list.
                let input_id = VideoInputId(LiveId(0));
                let format_id = VideoFormatId(LiveId(0));

                cx.prepare_video_playback(
                    LiveId(video_id),
                    PlatformVideoSource::Camera(input_id, format_id),
                    makepad_widgets::makepad_platform::event::video_playback::CameraPreviewMode::Texture,
                    0,
                    texture.texture_id(),
                    true,  // autoplay
                    false, // no loop (live stream)
                );

                self.streams.insert(stream_id, ActiveStream { video_id, image_key });

                log!(
                    "[camera] opened stream_id={} video_id={} key={:?}",
                    stream_id, video_id, image_key,
                );

                let _ = response.send(Ok(CameraStreamInfo {
                    stream_id,
                    image_key,
                    // Dimensions will be known after VideoPlaybackPrepared;
                    // use 0 as placeholder — script updates on the prepared event.
                    width: 0,
                    height: 0,
                }));
            },
            CameraRequest::Close(stream_id) => {
                if let Some(active) = self.streams.remove(&stream_id) {
                    log!("[camera] closing stream_id={} video_id={}", stream_id, active.video_id);
                    havi_render::video_texture_map::deregister_video_texture(active.image_key);
                    cx.cleanup_video_playback_resources(LiveId(active.video_id));
                }
            },
        }
    }
}
