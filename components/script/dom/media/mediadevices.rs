/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::rc::Rc;

use dom_struct::dom_struct;
use embedder_traits::{CameraRequest, EmbedderMsg};
use servo_media::ServoMedia;
use servo_media::streams::MediaStreamType;
use servo_media::streams::capture::MediaTrackConstraintSet;

use crate::dom::bindings::codegen::Bindings::MediaDevicesBinding::{
    MediaDevicesMethods, MediaStreamConstraints,
};
use crate::dom::bindings::codegen::UnionTypes::{
    BooleanOrMediaTrackConstraints, ClampedUnsignedLongOrConstrainULongRange as ConstrainULong,
    DoubleOrConstrainDoubleRange as ConstrainDouble,
};
use crate::dom::bindings::error::Error;
use crate::dom::bindings::reflector::{DomGlobal, reflect_dom_object};
use crate::dom::bindings::root::DomRoot;
use crate::dom::eventtarget::EventTarget;
use crate::dom::globalscope::GlobalScope;
use crate::dom::media::mediadeviceinfo::MediaDeviceInfo;
use crate::dom::media::mediastream::MediaStream;
use crate::dom::media::mediastreamtrack::MediaStreamTrack;
use crate::dom::promise::Promise;
use crate::realms::{AlreadyInRealm, InRealm};
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct MediaDevices {
    eventtarget: EventTarget,
}

impl MediaDevices {
    pub(crate) fn new_inherited() -> MediaDevices {
        MediaDevices {
            eventtarget: EventTarget::new_inherited(),
        }
    }

    pub(crate) fn new(global: &GlobalScope, can_gc: CanGc) -> DomRoot<MediaDevices> {
        reflect_dom_object(Box::new(MediaDevices::new_inherited()), global, can_gc)
    }
}

impl MediaDevicesMethods<crate::DomTypeHolder> for MediaDevices {
    /// <https://w3c.github.io/mediacapture-main/#dom-mediadevices-getusermedia>
    fn GetUserMedia(
        &self,
        constraints: &MediaStreamConstraints,
        comp: InRealm,
        can_gc: CanGc,
    ) -> Rc<Promise> {
        let p = Promise::new_in_current_realm(comp, can_gc);
        let global = self.global();
        let stream = MediaStream::new(&global, can_gc);

        // Audio: keep using servo-media DummyBackend for audio input streams.
        if let Some(audio_constraints) = convert_constraints(&constraints.audio) {
            let media = ServoMedia::get();
            if let Some(audio) = media.create_audioinput_stream(audio_constraints) {
                let track =
                    MediaStreamTrack::new(&global, audio, MediaStreamType::Audio, can_gc);
                stream.add_track(&track);
            }
        }

        // Video: use embedder camera channel.
        let wants_video = match &constraints.video {
            BooleanOrMediaTrackConstraints::Boolean(b) => *b,
            BooleanOrMediaTrackConstraints::MediaTrackConstraints(_) => true,
        };

        if wants_video {
            let webview_id = match global.webview_id() {
                Some(id) => id,
                None => {
                    p.reject_error(Error::NotSupported(None), can_gc);
                    return p;
                },
            };

            // Extract requested dimensions from constraints.
            let (width, height, frame_rate) = extract_video_constraints(&constraints.video);

            let (tx, rx) = crossbeam_channel::bounded(1);
            global.send_to_embedder(EmbedderMsg::CameraRequest(
                webview_id,
                CameraRequest::Open {
                    device_id: None,
                    width,
                    height,
                    frame_rate,
                    response: tx,
                },
            ));

            // Block on response. The embedder processes this synchronously
            // via Makepad action dispatch on the main thread. In single-process
            // mode this completes immediately after the event loop processes
            // the action.
            match rx.recv() {
                Ok(Ok(info)) => {
                    let track = MediaStreamTrack::new_camera(
                        &global,
                        info.stream_id,
                        info.image_key,
                        info.input_id,
                        info.format_id,
                        info.width,
                        info.height,
                        info.frame_rate,
                        String::new(),
                        can_gc,
                    );
                    stream.add_track(&track);
                },
                Ok(Err(err)) => {
                    log::warn!("getUserMedia camera open failed: {}", err);
                    p.reject_error(Error::NotFound(None), can_gc);
                    return p;
                },
                Err(_) => {
                    p.reject_error(Error::Abort(None), can_gc);
                    return p;
                },
            }
        }

        p.resolve_native(&stream, can_gc);
        p
    }

    /// <https://w3c.github.io/mediacapture-main/#dom-mediadevices-enumeratedevices>
    fn EnumerateDevices(&self, can_gc: CanGc) -> Rc<Promise> {
        let in_realm_proof = AlreadyInRealm::assert::<crate::DomTypeHolder>();
        let p = Promise::new_in_current_realm(InRealm::Already(&in_realm_proof), can_gc);
        let global = self.global();

        let webview_id = match global.webview_id() {
            Some(id) => id,
            None => {
                p.resolve_native(&Vec::<DomRoot<MediaDeviceInfo>>::new(), can_gc);
                return p;
            },
        };

        let (tx, rx) = crossbeam_channel::bounded(1);
        global.send_to_embedder(EmbedderMsg::CameraRequest(
            webview_id,
            CameraRequest::EnumerateDevices(tx),
        ));

        let result_list = match rx.recv() {
            Ok(devices) => devices
                .iter()
                .map(|device| {
                    MediaDeviceInfo::new(
                        &global,
                        &device.device_id,
                        crate::conversions::Convert::convert(
                            servo_media::streams::device_monitor::MediaDeviceKind::VideoInput,
                        ),
                        &device.label,
                        "",
                        can_gc,
                    )
                })
                .collect::<Vec<_>>(),
            Err(_) => vec![],
        };

        p.resolve_native(&result_list, can_gc);
        p
    }
}

fn extract_video_constraints(js: &BooleanOrMediaTrackConstraints) -> (u32, u32, f64) {
    match js {
        BooleanOrMediaTrackConstraints::Boolean(_) => (640, 480, 30.0),
        BooleanOrMediaTrackConstraints::MediaTrackConstraints(c) => {
            let width = c
                .parent
                .width
                .as_ref()
                .and_then(extract_culong_value)
                .unwrap_or(640);
            let height = c
                .parent
                .height
                .as_ref()
                .and_then(extract_culong_value)
                .unwrap_or(480);
            let fps = c
                .parent
                .frameRate
                .as_ref()
                .and_then(extract_cdouble_value)
                .unwrap_or(30.0);
            (width, height, fps)
        },
    }
}

fn extract_culong_value(js: &ConstrainULong) -> Option<u32> {
    match js {
        ConstrainULong::ClampedUnsignedLong(val) => Some(*val),
        ConstrainULong::ConstrainULongRange(range) => {
            range.ideal.or(range.exact).or(range.parent.max)
        },
    }
}

fn extract_cdouble_value(js: &ConstrainDouble) -> Option<f64> {
    match js {
        ConstrainDouble::Double(val) => Some(**val),
        ConstrainDouble::ConstrainDoubleRange(range) => {
            range.ideal.map(|x| *x).or(range.exact.map(|x| *x)).or(range.parent.max.map(|x| *x))
        },
    }
}

fn convert_constraints(js: &BooleanOrMediaTrackConstraints) -> Option<MediaTrackConstraintSet> {
    match js {
        BooleanOrMediaTrackConstraints::Boolean(false) => None,
        BooleanOrMediaTrackConstraints::Boolean(true) => Some(Default::default()),
        BooleanOrMediaTrackConstraints::MediaTrackConstraints(c) => Some(MediaTrackConstraintSet {
            height: c.parent.height.as_ref().and_then(convert_culong),
            width: c.parent.width.as_ref().and_then(convert_culong),
            aspect: c.parent.aspectRatio.as_ref().and_then(convert_cdouble),
            frame_rate: c.parent.frameRate.as_ref().and_then(convert_cdouble),
            sample_rate: c.parent.sampleRate.as_ref().and_then(convert_culong),
        }),
    }
}

fn convert_culong(js: &ConstrainULong) -> Option<servo_media::streams::capture::Constrain<u32>> {
    use servo_media::streams::capture::{Constrain, ConstrainRange};
    match js {
        ConstrainULong::ClampedUnsignedLong(val) => Some(Constrain::Value(*val)),
        ConstrainULong::ConstrainULongRange(range) => {
            if range.parent.min.is_some() || range.parent.max.is_some() {
                Some(Constrain::Range(ConstrainRange {
                    min: range.parent.min,
                    max: range.parent.max,
                    ideal: range.ideal,
                }))
            } else {
                range.exact.map(Constrain::Value)
            }
        },
    }
}

fn convert_cdouble(js: &ConstrainDouble) -> Option<servo_media::streams::capture::Constrain<f64>> {
    use servo_media::streams::capture::{Constrain, ConstrainRange};
    match js {
        ConstrainDouble::Double(val) => Some(Constrain::Value(**val)),
        ConstrainDouble::ConstrainDoubleRange(range) => {
            if range.parent.min.is_some() || range.parent.max.is_some() {
                Some(Constrain::Range(ConstrainRange {
                    min: range.parent.min.map(|x| *x),
                    max: range.parent.max.map(|x| *x),
                    ideal: range.ideal.map(|x| *x),
                }))
            } else {
                range.exact.map(|exact| Constrain::Value(*exact))
            }
        },
    }
}
