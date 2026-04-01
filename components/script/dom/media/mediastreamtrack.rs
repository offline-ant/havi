/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;

use dom_struct::dom_struct;
use embedder_traits::{CameraStreamId, EmbedderMsg};
use servo_media::streams::MediaStreamType;
use servo_media::streams::registry::MediaStreamId;

use crate::dom::bindings::codegen::Bindings::MediaStreamTrackBinding::MediaStreamTrackMethods;
use crate::dom::bindings::reflector::{DomGlobal, reflect_dom_object};
use crate::dom::bindings::root::DomRoot;
use crate::dom::bindings::str::DOMString;
use crate::dom::eventtarget::EventTarget;
use crate::dom::globalscope::GlobalScope;
use crate::script_runtime::CanGc;

/// The backing media source for a track.
///
/// Each variant carries the minimum state needed for its source type.
/// The renderer uses the image key to look up a Makepad texture;
/// script never touches pixel data.
#[derive(Clone, Debug)]
pub(crate) enum TrackSource {
    /// Dummy / servo-media stream (audio DummyBackend, etc).
    ServoMedia,
    /// Live camera capture via Makepad's video playback path.
    Camera {
        stream_id: CameraStreamId,
        /// Raw (namespace, index) image key registered in VideoTextureMap.
        image_key: (u32, u32),
        /// Backend camera source ids and format metadata used by MediaRecorder.
        input_id: u64,
        format_id: u64,
        width: u32,
        height: u32,
        frame_rate: f64,
    },
    // Future variants:
    // ScreenCapture { stream_id: ..., image_key: (u32, u32) },
    // Canvas { image_key: (u32, u32) },
    // Codec { decoder_id: ..., image_key: (u32, u32) },
}

impl TrackSource {
    /// Image key for the texture backing this track, if any.
    pub(crate) fn image_key(&self) -> Option<(u32, u32)> {
        match self {
            TrackSource::ServoMedia => None,
            TrackSource::Camera { image_key, .. } => Some(*image_key),
        }
    }
}

#[dom_struct]
pub(crate) struct MediaStreamTrack {
    eventtarget: EventTarget,
    #[ignore_malloc_size_of = "defined in servo-media"]
    #[no_trace]
    id: MediaStreamId,
    #[ignore_malloc_size_of = "defined in servo-media"]
    #[no_trace]
    ty: MediaStreamType,
    /// The backing source for this track.
    #[ignore_malloc_size_of = "enum"]
    #[no_trace]
    source: TrackSource,
    /// Track label (device name or empty).
    #[ignore_malloc_size_of = "String"]
    label: String,
    /// Whether the track is live or ended.
    ready_state_live: Cell<bool>,
    /// Whether the track is enabled (when false, frames are suppressed).
    enabled: Cell<bool>,
}

impl MediaStreamTrack {
    pub(crate) fn new_inherited(id: MediaStreamId, ty: MediaStreamType) -> MediaStreamTrack {
        MediaStreamTrack {
            eventtarget: EventTarget::new_inherited(),
            id,
            ty,
            source: TrackSource::ServoMedia,
            label: String::new(),
            ready_state_live: Cell::new(true),
            enabled: Cell::new(true),
        }
    }

    pub(crate) fn new(
        global: &GlobalScope,
        id: MediaStreamId,
        ty: MediaStreamType,
        can_gc: CanGc,
    ) -> DomRoot<MediaStreamTrack> {
        reflect_dom_object(
            Box::new(MediaStreamTrack::new_inherited(id, ty)),
            global,
            can_gc,
        )
    }

    /// Create a camera-backed video track.
    pub(crate) fn new_camera(
        global: &GlobalScope,
        stream_id: CameraStreamId,
        image_key: (u32, u32),
        input_id: u64,
        format_id: u64,
        width: u32,
        height: u32,
        frame_rate: f64,
        label: String,
        can_gc: CanGc,
    ) -> DomRoot<MediaStreamTrack> {
        let mut track = MediaStreamTrack::new_inherited(
            MediaStreamId::new(),
            MediaStreamType::Video,
        );
        track.source = TrackSource::Camera {
            stream_id,
            image_key,
            input_id,
            format_id,
            width,
            height,
            frame_rate,
        };
        track.label = label;
        reflect_dom_object(Box::new(track), global, can_gc)
    }

    pub(crate) fn id(&self) -> MediaStreamId {
        self.id
    }

    pub(crate) fn ty(&self) -> MediaStreamType {
        self.ty
    }

    pub(crate) fn source(&self) -> &TrackSource {
        &self.source
    }

    pub(crate) fn is_live(&self) -> bool {
        self.ready_state_live.get()
    }

    /// Stop the track: release the backing source, set state to ended.
    pub(crate) fn stop_track(&self) {
        if !self.ready_state_live.get() {
            return;
        }
        self.ready_state_live.set(false);

        match &self.source {
            TrackSource::Camera { stream_id, .. } => {
                if let Some(webview_id) = self.global().webview_id() {
                    self.global().send_to_embedder(
                        EmbedderMsg::CameraRequest(
                            webview_id,
                            embedder_traits::CameraRequest::Close(*stream_id),
                        ),
                    );
                }
            },
            TrackSource::ServoMedia => {},
        }
    }
}

impl MediaStreamTrackMethods<crate::DomTypeHolder> for MediaStreamTrack {
    /// <https://w3c.github.io/mediacapture-main/#dom-mediastreamtrack-kind>
    fn Kind(&self) -> DOMString {
        match self.ty {
            MediaStreamType::Video => "video".into(),
            MediaStreamType::Audio => "audio".into(),
        }
    }

    /// <https://w3c.github.io/mediacapture-main/#dom-mediastreamtrack-id>
    fn Id(&self) -> DOMString {
        self.id.id().to_string().into()
    }

    /// <https://w3c.github.io/mediacapture-main/#dom-mediastreamtrack-label>
    fn Label(&self) -> DOMString {
        DOMString::from(self.label.clone())
    }

    /// <https://w3c.github.io/mediacapture-main/#dom-mediastreamtrack-readystate>
    fn ReadyState(&self) -> DOMString {
        if self.ready_state_live.get() {
            "live".into()
        } else {
            "ended".into()
        }
    }

    /// <https://w3c.github.io/mediacapture-main/#dom-mediastreamtrack-enabled>
    fn Enabled(&self) -> bool {
        self.enabled.get()
    }

    /// <https://w3c.github.io/mediacapture-main/#dom-mediastreamtrack-enabled>
    fn SetEnabled(&self, value: bool) {
        self.enabled.set(value);
    }

    /// <https://w3c.github.io/mediacapture-main/#dom-mediastreamtrack-stop>
    fn Stop(&self) {
        self.stop_track();
    }

    /// <https://w3c.github.io/mediacapture-main/#dom-mediastreamtrack-clone>
    fn Clone(&self) -> DomRoot<MediaStreamTrack> {
        let mut track = MediaStreamTrack::new_inherited(self.id, self.ty);
        track.source = self.source.clone();
        track.label = self.label.clone();
        reflect_dom_object(Box::new(track), &*self.global(), CanGc::note())
    }
}
