/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use dom_struct::dom_struct;
use js::rust::HandleObject;
use servo_url::BrowserUrl;
use stylo_atoms::Atom;

use crate::dom::bindings::cell::DomRefCell;
use crate::dom::bindings::codegen::Bindings::MediaSourceBinding::MediaSourceMethods;
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::{DomGlobal, reflect_dom_object_with_proto};
use crate::dom::bindings::refcounted::Trusted;
use crate::dom::bindings::root::{Dom, DomRoot, MutNullableDom};
use crate::dom::bindings::str::DOMString;
use crate::dom::eventtarget::EventTarget;
use crate::dom::globalscope::GlobalScope;
use crate::dom::html::htmlmediaelement::HTMLMediaElement;
use crate::dom::media::sourcebuffer::SourceBuffer;
use crate::dom::media::sourcebufferlist::SourceBufferList;
use crate::dom::window::Window;
use crate::script_runtime::CanGc;
use media::controller::{self, MediaEvent};

static MEDIA_SOURCE_OBJECT_URLS: LazyLock<Mutex<HashMap<String, Trusted<MediaSource>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

const READY_STATE_CLOSED: u8 = 0;
const READY_STATE_OPEN: u8 = 1;
const READY_STATE_ENDED: u8 = 2;

fn ready_state_name(state: u8) -> &'static str {
    match state {
        READY_STATE_OPEN => "open",
        READY_STATE_ENDED => "ended",
        _ => "closed",
    }
}

#[dom_struct]
pub(crate) struct MediaSource {
    eventtarget: EventTarget,
    ready_state: Cell<u8>,
    duration: Cell<f64>,
    explicit_duration: Cell<bool>,
    source_buffers: DomRefCell<Vec<Dom<SourceBuffer>>>,
    source_buffers_list: MutNullableDom<SourceBufferList>,
    active_source_buffers_list: MutNullableDom<SourceBufferList>,
    attached_element: MutNullableDom<HTMLMediaElement>,
    attached_video_id: Cell<Option<u64>>,
}

impl MediaSource {
    fn new_inherited() -> Self {
        Self {
            eventtarget: EventTarget::new_inherited(),
            ready_state: Cell::new(READY_STATE_CLOSED),
            duration: Cell::new(f64::NAN),
            explicit_duration: Cell::new(false),
            source_buffers: DomRefCell::new(Vec::new()),
            source_buffers_list: Default::default(),
            active_source_buffers_list: Default::default(),
            attached_element: Default::default(),
            attached_video_id: Cell::new(None),
        }
    }

    fn new(global: &GlobalScope, proto: Option<HandleObject>, can_gc: CanGc) -> DomRoot<Self> {
        reflect_dom_object_with_proto(Box::new(Self::new_inherited()), global, proto, can_gc)
    }

    fn fire_simple_event(&self, name: &str, can_gc: CanGc) {
        self.upcast::<EventTarget>()
            .fire_event(Atom::from(name), can_gc);
    }

    fn is_type_supported_impl(mime: &str) -> bool {
        let base = mime.split(';').next().unwrap_or("").trim();
        matches!(base, "video/mp4" | "video/x-m4v" | "audio/mp4" | "audio/x-m4a")
            && controller::can_play_type(mime) != ""
    }

    fn duration_changed(old: f64, new: f64) -> bool {
        !(old.is_nan() && new.is_nan()) && old != new
    }

    fn source_buffers_list(&self, can_gc: CanGc) -> DomRoot<SourceBufferList> {
        let global = self.global();
        let window = global.as_window();
        self.source_buffers_list
            .or_init(|| SourceBufferList::new(window, &[], can_gc))
    }

    fn active_source_buffers_list(&self, can_gc: CanGc) -> DomRoot<SourceBufferList> {
        let global = self.global();
        let window = global.as_window();
        self.active_source_buffers_list
            .or_init(|| SourceBufferList::new(window, &[], can_gc))
    }

    fn sync_source_buffer_lists(&self, can_gc: CanGc) {
        let source_buffers: Vec<Dom<SourceBuffer>> = self.source_buffers.borrow().iter().cloned().collect();
        self.source_buffers_list(can_gc)
            .replace_all(source_buffers.clone());

        let active_source_buffers = if self.ready_state.get() == READY_STATE_CLOSED {
            Vec::new()
        } else {
            source_buffers
        };
        self.active_source_buffers_list(can_gc)
            .replace_all(active_source_buffers);
    }

    fn set_duration_value(&self, duration: f64, explicit: bool) {
        let old_duration = self.duration.replace(duration);
        if explicit {
            self.explicit_duration.set(true);
        }
        if Self::duration_changed(old_duration, duration) {
            if let Some(element) = self.attached_element.get() {
                element.apply_media_source_duration(duration);
            }
        }
    }

    fn update_duration_from_buffered_ranges(&self, buffered_ranges: &[(f64, f64)]) {
        if self.explicit_duration.get() {
            return;
        }
        let max_end = buffered_ranges
            .iter()
            .map(|(_, end)| *end)
            .fold(f64::NAN, f64::max);
        if !max_end.is_nan() {
            self.set_duration_value(max_end, false);
        }
    }

    pub(crate) fn effective_media_element_duration(&self, prepared_duration_ms: u128) -> f64 {
        let duration = self.duration.get();
        if self.explicit_duration.get() || !duration.is_nan() {
            return duration;
        }
        if prepared_duration_ms == 0 {
            f64::INFINITY
        } else {
            prepared_duration_ms as f64 / 1000.0
        }
    }

    pub(crate) fn register_object_url(url: String, source: &MediaSource) {
        MEDIA_SOURCE_OBJECT_URLS
            .lock()
            .unwrap()
            .insert(url, Trusted::new(source));
    }

    pub(crate) fn revoke_object_url(url: &BrowserUrl) {
        MEDIA_SOURCE_OBJECT_URLS.lock().unwrap().remove(url.as_str());
    }

    pub(crate) fn from_object_url(url: &BrowserUrl) -> Option<DomRoot<Self>> {
        MEDIA_SOURCE_OBJECT_URLS
            .lock()
            .unwrap()
            .get(url.as_str())
            .cloned()
            .map(|trusted| trusted.root())
    }

    pub(crate) fn attach_to_element(
        &self,
        element: &HTMLMediaElement,
        can_gc: CanGc,
    ) -> Result<(), ()> {
        self.attached_element.set(Some(element));
        self.ready_state.set(READY_STATE_OPEN);
        self.sync_source_buffer_lists(can_gc);
        if !self.duration.get().is_nan() {
            element.apply_media_source_duration(self.duration.get());
        }
        self.fire_simple_event("sourceopen", can_gc);
        self.ensure_playback_controller(can_gc).map_err(|_| ())
    }

    pub(crate) fn detach_from_element(&self, can_gc: CanGc) {
        self.attached_element.set(None);
        self.attached_video_id.set(None);
        if self.ready_state.get() != READY_STATE_CLOSED {
            self.ready_state.set(READY_STATE_CLOSED);
            self.sync_source_buffer_lists(can_gc);
            self.fire_simple_event("sourceclose", can_gc);
        } else {
            self.sync_source_buffer_lists(can_gc);
        }
    }

    pub(crate) fn attached_video_id(&self) -> Option<u64> {
        self.attached_video_id.get()
    }

    pub(crate) fn ensure_playback_controller(&self, can_gc: CanGc) -> Fallible<()> {
        if self.attached_video_id.get().is_some() {
            return Ok(());
        }
        let Some(element) = self.attached_element.get() else {
            return Ok(());
        };
        let Some(source_buffer) = self
            .source_buffers
            .borrow()
            .first()
            .map(|source_buffer| DomRoot::from_ref(&**source_buffer))
        else {
            return Ok(());
        };
        let mime = source_buffer.mime_type().str().to_string();
        let video_id = element.create_mse_media_player(self, mime)?;
        self.attached_video_id.set(Some(video_id));
        if self.ready_state.get() == READY_STATE_ENDED {
            self.ready_state.set(READY_STATE_OPEN);
            self.sync_source_buffer_lists(can_gc);
        }
        Ok(())
    }

    pub(crate) fn handle_media_event(&self, event: &MediaEvent, can_gc: CanGc) {
        let Some(source_buffer) = self
            .source_buffers
            .borrow()
            .first()
            .map(|source_buffer| DomRoot::from_ref(&**source_buffer))
        else {
            return;
        };
        match event {
            MediaEvent::MseAppendDone { buffered_ranges } => {
                source_buffer.notify_update_success(buffered_ranges, can_gc);
                self.update_duration_from_buffered_ranges(buffered_ranges);
            }
            MediaEvent::MseInitSegmentParsed { duration_ms, .. } => {
                if !self.explicit_duration.get() && *duration_ms != 0 {
                    self.set_duration_value(*duration_ms as f64 / 1000.0, false);
                }
            }
            MediaEvent::MseError(_) => {
                source_buffer.notify_update_error(can_gc);
            }
            _ => {}
        }
    }
}

impl MediaSourceMethods<crate::DomTypeHolder> for MediaSource {
    fn Constructor(global: &Window, proto: Option<HandleObject>, can_gc: CanGc) -> DomRoot<Self> {
        Self::new(&global.global(), proto, can_gc)
    }

    fn IsTypeSupported(_global: &Window, type_: DOMString) -> bool {
        Self::is_type_supported_impl(&type_.str())
    }

    fn SourceBuffers(&self) -> DomRoot<SourceBufferList> {
        self.sync_source_buffer_lists(CanGc::note());
        self.source_buffers_list(CanGc::note())
    }

    fn ActiveSourceBuffers(&self) -> DomRoot<SourceBufferList> {
        self.sync_source_buffer_lists(CanGc::note());
        self.active_source_buffers_list(CanGc::note())
    }

    fn ReadyState(&self) -> DOMString {
        DOMString::from(ready_state_name(self.ready_state.get()))
    }

    fn GetDuration(&self) -> Fallible<f64> {
        Ok(self.duration.get())
    }

    fn SetDuration(&self, duration: f64) -> ErrorResult {
        if self.ready_state.get() != READY_STATE_OPEN {
            return Err(Error::InvalidState(Some(
                "MediaSource is not open".into(),
            )));
        }
        if duration.is_nan() || duration < 0.0 {
            return Err(Error::InvalidAccess(Some(
                "MediaSource duration must be non-negative".into(),
            )));
        }
        if self
            .source_buffers
            .borrow()
            .iter()
            .any(|source_buffer| source_buffer.is_updating())
        {
            return Err(Error::InvalidState(Some(
                "SourceBuffer is updating".into(),
            )));
        }
        self.set_duration_value(duration, true);
        Ok(())
    }

    fn AddSourceBuffer(&self, type_: DOMString) -> Fallible<DomRoot<SourceBuffer>> {
        if self.ready_state.get() != READY_STATE_OPEN {
            return Err(Error::InvalidState(Some(
                "MediaSource is not open".into(),
            )));
        }
        if !Self::is_type_supported_impl(&type_.str()) {
            return Err(Error::NotSupported(Some(
                "unsupported MediaSource MIME type".into(),
            )));
        }
        if !self.source_buffers.borrow().is_empty() {
            return Err(Error::NotSupported(Some(
                "only one SourceBuffer is currently supported".into(),
            )));
        }
        let source_buffer = SourceBuffer::new(
            &self.global(),
            None,
            self,
            type_.clone(),
            CanGc::note(),
        );
        self.source_buffers
            .borrow_mut()
            .push(Dom::from_ref(&*source_buffer));
        self.sync_source_buffer_lists(CanGc::note());
        self.ensure_playback_controller(CanGc::note())?;
        Ok(source_buffer)
    }

    fn EndOfStream(&self) -> ErrorResult {
        if self.ready_state.get() != READY_STATE_OPEN {
            return Err(Error::InvalidState(Some(
                "MediaSource is not open".into(),
            )));
        }
        if self
            .source_buffers
            .borrow()
            .iter()
            .any(|source_buffer| source_buffer.is_updating())
        {
            return Err(Error::InvalidState(Some(
                "SourceBuffer is updating".into(),
            )));
        }
        if let Some(video_id) = self.attached_video_id.get() {
            controller::end_mse_stream(video_id);
        }
        self.ready_state.set(READY_STATE_ENDED);
        self.sync_source_buffer_lists(CanGc::note());
        self.fire_simple_event("sourceended", CanGc::note());
        Ok(())
    }

    event_handler!(sourceopen, GetOnsourceopen, SetOnsourceopen);
    event_handler!(sourceended, GetOnsourceended, SetOnsourceended);
    event_handler!(sourceclose, GetOnsourceclose, SetOnsourceclose);
}
