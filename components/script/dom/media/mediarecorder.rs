/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;

use dom_struct::dom_struct;
use js::rust::HandleObject;

use crate::dom::bindings::cell::DomRefCell;
use crate::dom::bindings::codegen::Bindings::MediaRecorderBinding::{
    MediaRecorderMethods, MediaRecorderOptions, RecordingState,
};
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::{DomGlobal, reflect_dom_object_with_proto};
use crate::dom::bindings::root::{Dom, DomRoot};
use crate::dom::bindings::str::DOMString;
use crate::dom::eventtarget::EventTarget;
use crate::dom::globalscope::GlobalScope;
use crate::dom::media::mediastream::MediaStream;
use crate::dom::window::Window;
use crate::script_runtime::CanGc;
use stylo_atoms::Atom;

#[dom_struct]
pub(crate) struct MediaRecorder {
    eventtarget: EventTarget,
    stream: Dom<MediaStream>,
    state: Cell<RecordingState>,
    mime_type: DomRefCell<DOMString>,
    last_timeslice: Cell<Option<u32>>,
}

impl MediaRecorder {
    fn new_inherited(stream: &MediaStream, mime_type: DOMString) -> Self {
        Self {
            eventtarget: EventTarget::new_inherited(),
            stream: Dom::from_ref(stream),
            state: Cell::new(RecordingState::Inactive),
            mime_type: DomRefCell::new(mime_type),
            last_timeslice: Cell::new(None),
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

    fn validate_options(options: &MediaRecorderOptions) -> Fallible<()> {
        if options.audioBitsPerSecond.is_some() ||
            options.videoBitsPerSecond.is_some() ||
            options.bitsPerSecond.is_some()
        {
            return Err(Self::not_supported(
                "audioBitsPerSecond/videoBitsPerSecond/bitsPerSecond are unsupported",
            ));
        }

        if !options.mimeType.is_empty() && !Self::is_type_supported_impl(&options.mimeType.str()) {
            return Err(Self::not_supported(
                "requested mimeType is not supported by current media policy",
            ));
        }

        Ok(())
    }

    fn fire_simple_event(&self, name: &str) {
        self.upcast::<EventTarget>()
            .fire_event(Atom::from(name), CanGc::note());
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
        Self::validate_options(options)?;
        Ok(Self::new(
            &global.global(),
            proto,
            stream,
            options.mimeType.clone(),
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
                "start() requires inactive state in part 1",
            ));
        }

        self.last_timeslice.set(timeslice);
        self.state.set(RecordingState::Recording);
        self.fire_simple_event("start");
        Ok(())
    }

    fn Stop(&self) -> ErrorResult {
        match self.state.get() {
            RecordingState::Inactive => {
                Err(Self::invalid_state("stop() requires an active recorder in part 1"))
            },
            RecordingState::Recording | RecordingState::Paused => {
                self.state.set(RecordingState::Inactive);
                self.fire_simple_event("stop");
                Ok(())
            },
        }
    }

    fn Pause(&self) -> ErrorResult {
        if self.state.get() == RecordingState::Inactive {
            return Err(Self::invalid_state(
                "pause() requires an active recorder in part 1",
            ));
        }
        Err(Self::not_supported(
            "pause() encoder control path is unsupported",
        ))
    }

    fn Resume(&self) -> ErrorResult {
        if self.state.get() == RecordingState::Inactive {
            return Err(Self::invalid_state(
                "resume() requires an active recorder in part 1",
            ));
        }
        Err(Self::not_supported(
            "resume() encoder control path is unsupported",
        ))
    }

    fn RequestData(&self) -> ErrorResult {
        if self.state.get() == RecordingState::Inactive {
            return Err(Self::invalid_state(
                "requestData() requires an active recorder in part 1",
            ));
        }
        Err(Self::not_supported(
            "requestData() chunk emission path is unsupported",
        ))
    }
}
