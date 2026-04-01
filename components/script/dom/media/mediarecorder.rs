/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;

use constellation_traits::BlobImpl;
use dom_struct::dom_struct;
use embedder_traits::{CameraRecordingEvent, CameraRequest, EmbedderMsg};
use js::rust::HandleObject;
use script_bindings::reflector::DomObject;
use servo_media::streams::MediaStreamType;
use stylo_atoms::Atom;

use crate::dom::bindings::cell::DomRefCell;
use crate::dom::bindings::codegen::Bindings::MediaRecorderBinding::{
    MediaRecorderMethods, MediaRecorderOptions, RecordingState,
};
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::{DomGlobal, reflect_dom_object_with_proto};
use crate::dom::bindings::refcounted::Trusted;
use crate::dom::bindings::root::{Dom, DomRoot};
use crate::dom::bindings::str::DOMString;
use crate::dom::blob::Blob;
use crate::dom::event::Event;
use crate::dom::eventtarget::EventTarget;
use crate::dom::globalscope::GlobalScope;
use crate::dom::media::mediastream::MediaStream;
use crate::dom::media::mediastreamtrack::TrackSource;
use crate::dom::messageevent::MessageEvent;
use crate::dom::window::Window;
use crate::script_runtime::CanGc;
use crate::task::TaskOnce;

const DEFAULT_RECORDER_MIME: &str = "video/mp4; codecs=\"av01.0.04M.08\"";

#[dom_struct]
pub(crate) struct MediaRecorder {
    eventtarget: EventTarget,
    stream: Dom<MediaStream>,
    state: Cell<RecordingState>,
    mime_type: DomRefCell<DOMString>,
    generation: Cell<u64>,
    camera_stream_id: Cell<Option<u64>>,
}

impl MediaRecorder {
    fn new_inherited(stream: &MediaStream, mime_type: DOMString) -> Self {
        Self {
            eventtarget: EventTarget::new_inherited(),
            stream: Dom::from_ref(stream),
            state: Cell::new(RecordingState::Inactive),
            mime_type: DomRefCell::new(mime_type),
            generation: Cell::new(0),
            camera_stream_id: Cell::new(None),
        }
    }

    fn new(
        global: &GlobalScope,
        proto: Option<HandleObject>,
        stream: &MediaStream,
        mime_type: DOMString,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object_with_proto(
            Box::new(Self::new_inherited(stream, mime_type)),
            global,
            proto,
            can_gc,
        )
    }

    fn is_type_supported_impl(mime_type: &str) -> bool {
        media::controller::can_play_type(mime_type) != ""
    }

    fn not_supported(message: &str) -> Error {
        Error::NotSupported(Some(format!("NotYetImplemented: {}", message)))
    }

    fn invalid_state(message: &str) -> Error {
        Error::InvalidState(Some(format!("NotYetImplemented: {}", message)))
    }

    fn validate_options(options: &MediaRecorderOptions) -> Fallible<DOMString> {
        if options.audioBitsPerSecond.is_some() ||
            options.videoBitsPerSecond.is_some() ||
            options.bitsPerSecond.is_some()
        {
            return Err(Self::not_supported(
                "audioBitsPerSecond/videoBitsPerSecond/bitsPerSecond are unsupported",
            ));
        }

        let resolved = if options.mimeType.is_empty() {
            DOMString::from(DEFAULT_RECORDER_MIME)
        } else {
            options.mimeType.clone()
        };

        if !Self::is_type_supported_impl(&resolved.str()) {
            return Err(Self::not_supported(
                "requested mimeType is not supported by current media policy",
            ));
        }

        Ok(resolved)
    }

    fn fire_simple_event(&self, name: &str) {
        self.upcast::<EventTarget>()
            .fire_event(Atom::from(name), CanGc::note());
    }

    fn next_generation(&self) -> u64 {
        let next = self.generation.get().wrapping_add(1);
        self.generation.set(next);
        next
    }

    fn extract_camera_stream_params(&self) -> Fallible<(u64, u64, u64, u32, u32, f64)> {
        let tracks = self.stream.get_tracks();
        let mut audio_tracks = 0u32;
        let mut video_tracks = 0u32;
        let mut camera: Option<(u64, u64, u64, u32, u32, f64)> = None;

        for track in tracks.iter() {
            match track.ty() {
                MediaStreamType::Audio => audio_tracks += 1,
                MediaStreamType::Video => {
                    video_tracks += 1;
                    if !track.is_live() {
                        return Err(Self::not_supported(
                            "video track must be live for MediaRecorder camera path",
                        ));
                    }
                    match track.source() {
                        TrackSource::Camera {
                            stream_id,
                            input_id,
                            format_id,
                            width,
                            height,
                            frame_rate,
                            ..
                        } => {
                            camera = Some((*stream_id, *input_id, *format_id, *width, *height, *frame_rate));
                        },
                        _ => {
                            return Err(Self::not_supported(
                                "only camera video tracks are supported for MediaRecorder part2",
                            ));
                        },
                    }
                },
            }
        }

        if audio_tracks > 0 {
            return Err(Self::not_supported(
                "audio-only and mixed audio/video recording paths are not implemented",
            ));
        }

        if video_tracks != 1 {
            return Err(Self::not_supported(
                "MediaRecorder part2 requires exactly one camera video track",
            ));
        }

        let Some(camera) = camera else {
            return Err(Self::not_supported(
                "MediaRecorder part2 camera stream details are unavailable",
            ));
        };

        if camera.3 == 0 || camera.4 == 0 || camera.5 <= 0.0 {
            return Err(Self::not_supported(
                "camera stream constraints are not supported for recorder start",
            ));
        }

        Ok(camera)
    }

    fn dispatch_dataavailable_chunk(&self, chunk: Vec<u8>, can_gc: CanGc) {
        if chunk.is_empty() {
            return;
        }

        let mime = self.mime_type.borrow().str().to_string();
        let blob_impl = BlobImpl::new_from_bytes(chunk, mime);
        let blob = Blob::new(&self.global(), blob_impl, can_gc);
        rooted!(in(*GlobalScope::get_cx()) let blob_val =
            js::jsval::ObjectValue(blob.reflector().get_jsobject().get()));

        let event = MessageEvent::new(
            &self.global(),
            Atom::from("dataavailable"),
            false,
            false,
            blob_val.handle(),
            DOMString::new(),
            None,
            DOMString::new(),
            vec![],
            can_gc,
        );
        event.upcast::<Event>().fire(self.upcast(), can_gc);
    }
}

impl MediaRecorderMethods<crate::DomTypeHolder> for MediaRecorder {
    /// <https://w3c.github.io/mediacapture-record/#constructors>
    fn Constructor(
        global: &Window,
        proto: Option<HandleObject>,
        can_gc: CanGc,
        stream: &MediaStream,
        options: &MediaRecorderOptions,
    ) -> Fallible<DomRoot<Self>> {
        let mime_type = Self::validate_options(options)?;
        Ok(Self::new(
            &global.global(),
            proto,
            stream,
            mime_type,
            can_gc,
        ))
    }

    fn IsTypeSupported(_global: &Window, mime_type: DOMString) -> bool {
        Self::is_type_supported_impl(&mime_type.str())
    }

    fn State(&self) -> RecordingState {
        self.state.get()
    }

    fn MimeType(&self) -> DOMString {
        self.mime_type.borrow().clone()
    }

    fn Stream(&self) -> DomRoot<MediaStream> {
        DomRoot::from_ref(&self.stream)
    }

    event_handler!(start, GetOnstart, SetOnstart);
    event_handler!(stop, GetOnstop, SetOnstop);
    event_handler!(dataavailable, GetOndataavailable, SetOndataavailable);
    event_handler!(error, GetOnerror, SetOnerror);

    fn Start(&self, timeslice: Option<u32>) -> ErrorResult {
        if self.state.get() != RecordingState::Inactive {
            return Err(Self::invalid_state(
                "start() requires inactive state",
            ));
        }

        let (camera_stream_id, _input_id, _format_id, _width, _height, _frame_rate) =
            self.extract_camera_stream_params()?;

        let webview_id = self
            .global()
            .webview_id()
            .ok_or_else(|| Self::not_supported("no webview context for camera recorder"))?;

        let timeslice_ms = timeslice.unwrap_or(200).max(1);
        let (event_sender, event_receiver) = crossbeam_channel::unbounded::<CameraRecordingEvent>();
        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);

        self.global().send_to_embedder(EmbedderMsg::CameraRequest(
            webview_id,
            CameraRequest::StartRecording {
                stream_id: camera_stream_id,
                mime_type: self.mime_type.borrow().str().to_string(),
                timeslice_ms,
                event_sender,
                response: response_sender,
            },
        ));

        match response_receiver.recv() {
            Ok(Ok(())) => {},
            Ok(Err(message)) => return Err(Self::not_supported(&message)),
            Err(_) => {
                return Err(Self::not_supported(
                    "recorder start response channel closed unexpectedly",
                ));
            },
        }

        let generation = self.next_generation();
        self.camera_stream_id.set(Some(camera_stream_id));
        self.state.set(RecordingState::Recording);

        let address = Trusted::new(self);
        let task_source = self
            .global()
            .task_manager()
            .dom_manipulation_task_source()
            .to_sendable();

        std::thread::spawn(move || {
            while let Ok(event) = event_receiver.recv() {
                match event {
                    CameraRecordingEvent::Chunk(chunk) => {
                        task_source.queue(MediaRecorderChunkTask {
                            address: address.clone(),
                            generation,
                            chunk,
                        });
                    },
                    CameraRecordingEvent::Error(message) => {
                        task_source.queue(MediaRecorderErrorTask {
                            address: address.clone(),
                            generation,
                            message,
                        });
                    },
                }
            }
        });

        self.fire_simple_event("start");
        Ok(())
    }

    fn Stop(&self) -> ErrorResult {
        match self.state.get() {
            RecordingState::Inactive => {
                Err(Self::invalid_state("stop() requires an active recorder"))
            },
            RecordingState::Recording | RecordingState::Paused => {
                let stream_id = self
                    .camera_stream_id
                    .get()
                    .ok_or_else(|| Self::invalid_state("recorder camera stream state is missing"))?;

                let webview_id = self
                    .global()
                    .webview_id()
                    .ok_or_else(|| Self::not_supported("no webview context for recorder stop"))?;

                // Invalidate asynchronous chunk tasks from the old run before stop ordering.
                self.next_generation();

                let (response_sender, response_receiver) = crossbeam_channel::bounded(1);
                self.global().send_to_embedder(EmbedderMsg::CameraRequest(
                    webview_id,
                    CameraRequest::StopRecording {
                        stream_id,
                        response: response_sender,
                    },
                ));

                let final_chunk = match response_receiver.recv() {
                    Ok(Ok(chunk)) => chunk,
                    Ok(Err(message)) => return Err(Self::not_supported(&message)),
                    Err(_) => {
                        return Err(Self::not_supported(
                            "recorder stop response channel closed unexpectedly",
                        ));
                    },
                };

                if let Some(chunk) = final_chunk {
                    self.dispatch_dataavailable_chunk(chunk, CanGc::note());
                }

                self.camera_stream_id.set(None);
                self.state.set(RecordingState::Inactive);
                self.fire_simple_event("stop");
                Ok(())
            },
        }
    }

    fn Pause(&self) -> ErrorResult {
        if self.state.get() == RecordingState::Inactive {
            return Err(Self::invalid_state(
                "pause() requires an active recorder",
            ));
        }
        Err(Self::not_supported(
            "pause() encoder control path is unsupported",
        ))
    }

    fn Resume(&self) -> ErrorResult {
        if self.state.get() == RecordingState::Inactive {
            return Err(Self::invalid_state(
                "resume() requires an active recorder",
            ));
        }
        Err(Self::not_supported(
            "resume() encoder control path is unsupported",
        ))
    }

    fn RequestData(&self) -> ErrorResult {
        if self.state.get() == RecordingState::Inactive {
            return Err(Self::invalid_state(
                "requestData() requires an active recorder",
            ));
        }
        Err(Self::not_supported(
            "requestData() explicit chunk request path is unsupported",
        ))
    }
}

struct MediaRecorderChunkTask {
    address: Trusted<MediaRecorder>,
    generation: u64,
    chunk: Vec<u8>,
}

impl TaskOnce for MediaRecorderChunkTask {
    fn run_once(self, cx: &mut js::context::JSContext) {
        let recorder = self.address.root();
        if recorder.generation.get() != self.generation ||
            recorder.state.get() != RecordingState::Recording
        {
            return;
        }
        recorder.dispatch_dataavailable_chunk(self.chunk, CanGc::from_cx(cx));
    }
}

struct MediaRecorderErrorTask {
    address: Trusted<MediaRecorder>,
    generation: u64,
    message: String,
}

impl TaskOnce for MediaRecorderErrorTask {
    fn run_once(self, _cx: &mut js::context::JSContext) {
        let recorder = self.address.root();
        if recorder.generation.get() != self.generation {
            return;
        }
        log::warn!("MediaRecorder encode/mux error: {}", self.message);
        recorder.fire_simple_event("error");
    }
}
