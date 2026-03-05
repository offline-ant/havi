//! Camera subsystem: translates CameraRequest embedder messages into
//! Makepad CxMediaApi / video playback calls.
//!
//! Camera is treated as "a video whose source is a device." Frames go
//! directly to a Makepad Texture via `prepare_video_playback` with
//! `VideoSource::Camera`, same as decoded video. Script only receives
//! an image_key and dimensions — no pixel data crosses this boundary.

use std::sync::{Arc, Mutex};

use makepad_media::{EncodedVideoPacketOwned, mux::build_av1_mp4};

use super::*;
use makepad_widgets::makepad_platform::video::{
    VideoCodec, VideoEncodeSource, VideoEncoderConfig, VideoFormatId, VideoInputId,
    VideoPixelFormat,
};
use makepad_widgets::makepad_platform::VideoQueuePolicy;
use servo::{CameraRecordingEvent, CameraRequest, CameraStreamInfo};

/// Shared packet buffer for one active recorder.
struct RecorderBuffer {
    running: bool,
    packets: Vec<EncodedVideoPacketOwned>,
}

struct RecorderSession {
    shared: Arc<Mutex<RecorderBuffer>>,
    worker: Option<std::thread::JoinHandle<()>>,
    width: u32,
    height: u32,
    fps_num: u32,
}

impl RecorderSession {
    fn stop(mut self) -> Result<Option<Vec<u8>>, String> {
        {
            let mut shared = self.shared.lock().unwrap();
            shared.running = false;
        }

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }

        let packets = {
            let mut shared = self.shared.lock().unwrap();
            std::mem::take(&mut shared.packets)
        };

        if packets.is_empty() {
            return Ok(None);
        }

        build_av1_mp4(
            self.width as u16,
            self.height as u16,
            self.fps_num,
            1,
            &packets,
        )
        .map(Some)
        .ok_or_else(|| "NotYetImplemented: failed to mux final AV1/MP4 chunk".to_string())
    }
}

/// Per-stream bookkeeping.
struct ActiveStream {
    /// Makepad video_id used with prepare_video_playback / cleanup.
    video_id: u64,
    /// Raw (namespace, index) image key in VideoTextureMap.
    image_key: (u32, u32),
    input_id: VideoInputId,
    format_id: VideoFormatId,
    width: u32,
    height: u32,
    frame_rate: f64,
    recorder_format_supported: bool,
    recorder: Option<RecorderSession>,
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
    /// Preferred recorder-capable backend source tuple (input, format, dimensions, fps).
    default_source: Option<(VideoInputId, VideoFormatId, u32, u32, f64)>,
    /// First available backend source tuple for preview fallback.
    fallback_source: Option<(VideoInputId, VideoFormatId, u32, u32, f64)>,
}

fn yuv_recorder_rank(pixel_format: VideoPixelFormat) -> Option<u8> {
    match pixel_format {
        VideoPixelFormat::NV12 => Some(3),
        VideoPixelFormat::YUY2 => Some(2),
        VideoPixelFormat::YUV420 => Some(1),
        _ => None,
    }
}

impl CameraState {
    pub(super) fn image_key_for_video_id(&self, video_id: u64) -> Option<(u32, u32)> {
        self.streams
            .values()
            .find(|stream| stream.video_id == video_id)
            .map(|stream| stream.image_key)
    }

    pub(super) fn handle_video_inputs_event(
        &mut self,
        ev: &VideoInputsEvent,
    ) {
        let mut devices = Vec::new();
        self.default_source = None;
        self.fallback_source = None;
        let mut best_rank = 0u8;
        let mut best_area = 0u64;
        let mut best_fps = 0f64;

        for desc in &ev.descs {
            let mut formats = Vec::new();
            for fmt in &desc.formats {
                let width = fmt.width as u32;
                let height = fmt.height as u32;
                let frame_rate = fmt.frame_rate.unwrap_or(30.0);

                if self.fallback_source.is_none() {
                    self.fallback_source = Some((
                        desc.input_id,
                        fmt.format_id,
                        width,
                        height,
                        frame_rate,
                    ));
                }

                if let Some(rank) = yuv_recorder_rank(fmt.pixel_format) {
                    let area = (width as u64) * (height as u64);
                    if self.default_source.is_none() ||
                        rank > best_rank ||
                        (rank == best_rank && area > best_area) ||
                        (rank == best_rank && area == best_area && frame_rate > best_fps)
                    {
                        self.default_source = Some((
                            desc.input_id,
                            fmt.format_id,
                            width,
                            height,
                            frame_rate,
                        ));
                        best_rank = rank;
                        best_area = area;
                        best_fps = frame_rate;
                    }
                }

                formats.push(servo::CameraFormat {
                    width,
                    height,
                    frame_rate,
                });
            }
            devices.push(servo::CameraDeviceInfo {
                device_id: desc.input_id.0 .0.to_string(),
                label: desc.name.clone(),
                formats,
            });
        }

        if self.default_source.is_none() {
            log!("[camera] no recorder-capable camera format found (need NV12/YUY2/YUV420)");
        }

        self.devices = Some(devices);
    }

    fn stop_stream_recorder(active: &mut ActiveStream) -> Result<Option<Vec<u8>>, String> {
        match active.recorder.take() {
            Some(recorder) => recorder.stop(),
            None => Ok(None),
        }
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
                havi_render::video_texture_map::set_external_texture(image_key, texture.clone());

                // Prefer recorder-capable YUV source. Fall back to any source for preview only.
                // TODO: match requested device_id/width/height/frame_rate.
                let (input_id, format_id, width, height, frame_rate, recorder_format_supported) =
                    if let Some((input_id, format_id, width, height, frame_rate)) = self.default_source {
                        (input_id, format_id, width, height, frame_rate, true)
                    } else if let Some((input_id, format_id, width, height, frame_rate)) = self.fallback_source {
                        (input_id, format_id, width, height, frame_rate, false)
                    } else {
                        (
                            VideoInputId(LiveId(0)),
                            VideoFormatId(LiveId(0)),
                            640,
                            480,
                            30.0,
                            false,
                        )
                    };

                cx.prepare_video_playback(
                    LiveId(video_id),
                    PlatformVideoSource::Camera(input_id, format_id),
                    makepad_widgets::makepad_platform::event::video_playback::CameraPreviewMode::Texture,
                    0,
                    texture.texture_id(),
                    true,  // autoplay
                    false, // no loop (live stream)
                );

                self.streams.insert(
                    stream_id,
                    ActiveStream {
                        video_id,
                        image_key,
                        input_id,
                        format_id,
                        width,
                        height,
                        frame_rate,
                        recorder_format_supported,
                        recorder: None,
                    },
                );

                log!(
                    "[camera] opened stream_id={} video_id={} key={:?}",
                    stream_id, video_id, image_key,
                );

                let _ = response.send(Ok(CameraStreamInfo {
                    stream_id,
                    image_key,
                    input_id: input_id.0 .0,
                    format_id: format_id.0 .0,
                    width,
                    height,
                    frame_rate,
                }));
            },
            CameraRequest::StartRecording {
                stream_id,
                mime_type,
                timeslice_ms,
                event_sender,
                response,
            } => {
                if media_controller::can_play_type(&mime_type).is_empty() {
                    let _ = response.send(Err(
                        "NotYetImplemented: unsupported MediaRecorder mimeType for camera path"
                            .to_string(),
                    ));
                    return;
                }

                let preferred_source = self.default_source;
                let Some(active) = self.streams.get_mut(&stream_id) else {
                    let _ = response.send(Err(
                        "NotYetImplemented: unknown camera stream for MediaRecorder".to_string(),
                    ));
                    return;
                };

                if active.recorder.is_some() {
                    let _ = response.send(Err(
                        "NotYetImplemented: camera recorder already active for this stream"
                            .to_string(),
                    ));
                    return;
                }

                if !active.recorder_format_supported {
                    if let Some((input_id, format_id, width, height, frame_rate)) = preferred_source {
                        active.input_id = input_id;
                        active.format_id = format_id;
                        active.width = width;
                        active.height = height;
                        active.frame_rate = frame_rate;
                        active.recorder_format_supported = true;
                    } else {
                        let _ = response.send(Err(
                            "NotYetImplemented: no encoder-friendly camera format (need NV12/YUY2/YUV420)"
                                .to_string(),
                        ));
                        return;
                    }
                }

                let mut width = active.width.max(2);
                let mut height = active.height.max(2);
                if width % 2 != 0 {
                    width += 1;
                }
                if height % 2 != 0 {
                    height += 1;
                }
                let fps_num = active.frame_rate.round().clamp(1.0, 240.0) as u32;
                let encoder_index = stream_id as usize;

                let shared = Arc::new(Mutex::new(RecorderBuffer {
                    running: true,
                    packets: Vec::new(),
                }));
                let shared_cb = shared.clone();

                cx.video_input(encoder_index, |_buf| {});

                let config = VideoEncoderConfig {
                    codec: VideoCodec::Av1,
                    source: VideoEncodeSource::Camera {
                        input_id: active.input_id,
                        format_id: active.format_id,
                    },
                    width,
                    height,
                    fps_num,
                    fps_den: 1,
                    target_bitrate: 2_000_000,
                    // Part2 bootstrap: force immediate keyframes for first chunk availability.
                    keyint: 1,
                    latency_realtime: true,
                    codec_mode: 8,
                    queue_policy: VideoQueuePolicy::LatestWins,
                    queue_capacity: 2,
                };

                let start_result =
                    cx.video_encoder_output_try(encoder_index, config, move |packet| {
                        let mut buffer = shared_cb.lock().unwrap();
                        if !buffer.running {
                            return;
                        }
                        buffer.packets.push(EncodedVideoPacketOwned {
                            codec: packet.codec,
                            format: packet.format,
                            pts_ns: packet.pts_ns,
                            dts_ns: packet.dts_ns,
                            is_key: packet.is_key,
                            is_config: packet.is_config,
                            is_eos: packet.is_eos,
                            config_id: packet.config_id,
                            data: packet.data.to_vec(),
                        });
                    });

                if let Err(err) = start_result {
                    let _ = response.send(Err(format!(
                        "NotYetImplemented: failed to start camera encoder: {:?}",
                        err
                    )));
                    return;
                }

                cx.use_video_input(&[(active.input_id, active.format_id)]);

                // Contract guard: unsupported camera/encoder combinations must fail fast.
                // If no encoded packets appear shortly after start, surface NYI instead of
                // leaving recorder in a chunk-timeout state.
                let mut saw_packets = false;
                for _ in 0..20 {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    let has_packets = {
                        let buffer = shared.lock().unwrap();
                        !buffer.packets.is_empty()
                    };
                    if has_packets {
                        saw_packets = true;
                        break;
                    }
                }

                if !saw_packets {
                    {
                        let mut buffer = shared.lock().unwrap();
                        buffer.running = false;
                        buffer.packets.clear();
                    }
                    let _ = response.send(Err(
                        "NotYetImplemented: no encoder-friendly camera format (no encoder packets produced)"
                            .to_string(),
                    ));
                    return;
                }

                let tick_ms = timeslice_ms.max(1);
                let shared_thread = shared.clone();
                let worker = std::thread::spawn(move || {
                    let mut pending_packets: Vec<EncodedVideoPacketOwned> = Vec::new();
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(tick_ms as u64));

                        let running = {
                            let mut buffer = shared_thread.lock().unwrap();
                            let running = buffer.running;
                            if !buffer.packets.is_empty() {
                                pending_packets.extend(std::mem::take(&mut buffer.packets));
                            }
                            running
                        };

                        if !running {
                            if !pending_packets.is_empty() {
                                let mut buffer = shared_thread.lock().unwrap();
                                buffer.packets.extend(pending_packets.drain(..));
                            }
                            break;
                        }

                        if pending_packets.is_empty() {
                            continue;
                        }

                        match build_av1_mp4(width as u16, height as u16, fps_num, 1, &pending_packets)
                        {
                            Some(chunk) => {
                                if event_sender
                                    .send(CameraRecordingEvent::Chunk(chunk))
                                    .is_err()
                                {
                                    let mut buffer = shared_thread.lock().unwrap();
                                    buffer.packets.extend(pending_packets.drain(..));
                                    break;
                                }
                                pending_packets.clear();
                            },
                            None => {
                                let key_count = pending_packets.iter().filter(|p| p.is_key).count();
                                let config_count =
                                    pending_packets.iter().filter(|p| p.is_config).count();
                                let eos_count = pending_packets.iter().filter(|p| p.is_eos).count();
                                let byte_count: usize =
                                    pending_packets.iter().map(|p| p.data.len()).sum();
                                log!(
                                    "[camera-rec] waiting mux-ready: packets={} key={} config={} eos={} bytes={}",
                                    pending_packets.len(),
                                    key_count,
                                    config_count,
                                    eos_count,
                                    byte_count,
                                );
                            },
                        }
                    }
                });

                active.recorder = Some(RecorderSession {
                    shared,
                    worker: Some(worker),
                    width,
                    height,
                    fps_num,
                });

                let _ = response.send(Ok(()));
            },
            CameraRequest::StopRecording {
                stream_id,
                response,
            } => {
                let Some(active) = self.streams.get_mut(&stream_id) else {
                    let _ = response.send(Err(
                        "NotYetImplemented: unknown camera stream for recorder stop".to_string(),
                    ));
                    return;
                };

                let _ = response.send(Self::stop_stream_recorder(active));
            },
            CameraRequest::Close(stream_id) => {
                if let Some(mut active) = self.streams.remove(&stream_id) {
                    let _ = Self::stop_stream_recorder(&mut active);
                    log!("[camera] closing stream_id={} video_id={}", stream_id, active.video_id);
                    havi_render::video_texture_map::remove_video_binding(active.image_key);
                    cx.cleanup_video_playback_resources(LiveId(active.video_id));
                }
            },
        }
    }
}
