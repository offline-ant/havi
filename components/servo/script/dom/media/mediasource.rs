/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use dom_struct::dom_struct;
use js::rust::HandleObject;
use servo_url::BrowserUrl;
use stylo_atoms::Atom;

use crate::script::dom::bindings::cell::DomRefCell;
use crate::script::dom::bindings::codegen::GenericBindings::HTMLMediaElementBinding::HTMLMediaElementMethods;
use crate::script::dom::bindings::codegen::GenericBindings::MediaSourceBinding::MediaSourceMethods;
use crate::script::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::script::dom::bindings::inheritance::Castable;
use crate::script::dom::bindings::reflector::{DomGlobal, reflect_dom_object_with_proto};
use crate::script::dom::bindings::refcounted::Trusted;
use crate::script::dom::bindings::root::{Dom, DomRoot, MutNullableDom};
use crate::script::dom::bindings::str::DOMString;
use crate::script::dom::eventtarget::EventTarget;
use crate::script::dom::globalscope::GlobalScope;
use crate::script::dom::html::htmlmediaelement::HTMLMediaElement;
use crate::script::dom::media::sourcebuffer::SourceBuffer;
use crate::script::dom::media::sourcebufferlist::SourceBufferList;
use crate::script::dom::window::Window;
use crate::script::script_runtime::CanGc;
use crate::media::controller::{self, MediaEvent, MseSourceBufferInputId};

static MEDIA_SOURCE_OBJECT_URLS: LazyLock<Mutex<HashMap<String, Trusted<MediaSource>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

const READY_STATE_CLOSED: u8 = 0;
const READY_STATE_OPEN: u8 = 1;
const READY_STATE_ENDED: u8 = 2;

const ATTACHMENT_NONE: u8 = 0;
const ATTACHMENT_OBJECT_URL: u8 = 1;
const ATTACHMENT_MEDIA_PROVIDER_OBJECT: u8 = 2;

#[derive(Clone, Copy, Default)]
struct SourceBufferTrackMetadata {
    has_video: bool,
    has_audio: bool,
}

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
    #[no_trace]
    #[ignore_malloc_size_of = "MSE track metadata"]
    track_metadata_by_input: DomRefCell<HashMap<MseSourceBufferInputId, SourceBufferTrackMetadata>>,
    attached_element: MutNullableDom<HTMLMediaElement>,
    attachment_kind: Cell<u8>,
    next_input_id: Cell<MseSourceBufferInputId>,
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
            track_metadata_by_input: DomRefCell::new(HashMap::new()),
            attached_element: Default::default(),
            attachment_kind: Cell::new(ATTACHMENT_NONE),
            next_input_id: Cell::new(1),
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

    fn live_source_buffers(&self) -> Vec<Dom<SourceBuffer>> {
        self.source_buffers.borrow().iter().cloned().collect()
    }

    fn sync_source_buffer_lists(&self, can_gc: CanGc) {
        let source_buffers = self.live_source_buffers();
        self.source_buffers_list(can_gc)
            .replace_all(source_buffers.clone());

        let active_source_buffers = if self.is_open() || self.is_ended() {
            self.compute_active_source_buffers(source_buffers, can_gc)
        } else {
            Vec::new()
        };
        self.active_source_buffers_list(can_gc)
            .replace_all(active_source_buffers);
    }

    fn compute_active_source_buffers(
        &self,
        source_buffers: Vec<Dom<SourceBuffer>>,
        can_gc: CanGc,
    ) -> Vec<Dom<SourceBuffer>> {
        let track_metadata = self.track_metadata_by_input.borrow();
        if track_metadata.is_empty() {
            return source_buffers;
        }

        let Some(element) = self.attached_element.get() else {
            return source_buffers;
        };
        let selected_video = element.VideoTracks(can_gc).selected_index().is_some();
        let enabled_audio = element.AudioTracks(can_gc).enabled_index().is_some();

        let mut active_input_ids = HashSet::new();
        for (&input_id, metadata) in track_metadata.iter() {
            if (metadata.has_video && selected_video) || (metadata.has_audio && enabled_audio) {
                active_input_ids.insert(input_id);
            }
        }
        if active_input_ids.is_empty() {
            return source_buffers;
        }

        source_buffers
            .into_iter()
            .filter(|source_buffer| active_input_ids.contains(&source_buffer.input_id()))
            .collect()
    }

    fn set_ready_state(&self, ready_state: u8, can_gc: CanGc) {
        if self.ready_state.replace(ready_state) != ready_state {
            self.sync_source_buffer_lists(can_gc);
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.ready_state.get() == READY_STATE_OPEN
    }

    fn is_ended(&self) -> bool {
        self.ready_state.get() == READY_STATE_ENDED
    }

    fn require_open(&self, context: &str) -> ErrorResult {
        if self.is_open() {
            Ok(())
        } else {
            Err(Error::InvalidState(Some(format!("MediaSource is not open during {context}"))))
        }
    }

    fn require_no_updating_source_buffers(&self, context: &str) -> ErrorResult {
        if self
            .source_buffers
            .borrow()
            .iter()
            .any(|source_buffer| source_buffer.is_updating())
        {
            return Err(Error::InvalidState(Some(format!(
                "SourceBuffer is updating during {context}"
            ))));
        }
        Ok(())
    }

    fn allocate_input_id(&self) -> MseSourceBufferInputId {
        let input_id = self.next_input_id.get();
        self.next_input_id.set(input_id.saturating_add(1));
        input_id
    }

    fn source_buffer_by_input_id(
        &self,
        input_id: MseSourceBufferInputId,
    ) -> Option<DomRoot<SourceBuffer>> {
        self.source_buffers.borrow().iter().find_map(|source_buffer| {
            (source_buffer.input_id() == input_id).then(|| DomRoot::from_ref(&**source_buffer))
        })
    }

    fn update_source_buffer_track_metadata(
        &self,
        input_id: MseSourceBufferInputId,
        video_tracks: &[String],
        audio_tracks: &[String],
        can_gc: CanGc,
    ) {
        self.track_metadata_by_input.borrow_mut().insert(
            input_id,
            SourceBufferTrackMetadata {
                has_video: !video_tracks.is_empty(),
                has_audio: !audio_tracks.is_empty(),
            },
        );
        self.sync_source_buffer_lists(can_gc);
    }

    pub(crate) fn media_track_selection_changed(&self, can_gc: CanGc) {
        self.sync_source_buffer_lists(can_gc);
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

    fn attach_to_element_with_kind(
        &self,
        element: &HTMLMediaElement,
        attachment_kind: u8,
        can_gc: CanGc,
    ) -> Result<(), ()> {
        if let Some(attached_element) = self.attached_element.get() {
            if !std::ptr::eq(&*attached_element, element) || self.attachment_kind.get() != attachment_kind {
                return Err(());
            }
            self.ensure_playback_controller(can_gc).map_err(|_| ())?;
            return Ok(());
        }

        self.attached_element.set(Some(element));
        self.attachment_kind.set(attachment_kind);
        self.set_ready_state(READY_STATE_OPEN, can_gc);
        if !self.duration.get().is_nan() {
            element.apply_media_source_duration(self.duration.get());
        }
        self.fire_simple_event("sourceopen", can_gc);
        self.ensure_playback_controller(can_gc).map_err(|_| ())
    }

    pub(crate) fn attach_to_element_via_object_url(
        &self,
        element: &HTMLMediaElement,
        can_gc: CanGc,
    ) -> Result<(), ()> {
        self.attach_to_element_with_kind(element, ATTACHMENT_OBJECT_URL, can_gc)
    }

    pub(crate) fn attach_to_element_via_media_provider_object(
        &self,
        element: &HTMLMediaElement,
        can_gc: CanGc,
    ) -> Result<(), ()> {
        self.attach_to_element_with_kind(element, ATTACHMENT_MEDIA_PROVIDER_OBJECT, can_gc)
    }

    pub(crate) fn detach_from_element(&self, can_gc: CanGc) {
        for source_buffer in self.source_buffers.borrow().iter() {
            source_buffer.clear_for_detach();
        }

        self.attached_element.set(None);
        self.attachment_kind.set(ATTACHMENT_NONE);
        self.attached_video_id.set(None);
        self.track_metadata_by_input.borrow_mut().clear();

        if self.ready_state.get() != READY_STATE_CLOSED {
            self.set_ready_state(READY_STATE_CLOSED, can_gc);
            self.fire_simple_event("sourceclose", can_gc);
        } else {
            self.sync_source_buffer_lists(can_gc);
        }
    }

    pub(crate) fn has_source_buffer(&self, source_buffer: &SourceBuffer) -> bool {
        self.source_buffers
            .borrow()
            .iter()
            .any(|existing| std::ptr::eq(&**existing, source_buffer))
    }

    pub(crate) fn attached_video_id(&self) -> Option<u64> {
        self.attached_video_id.get()
    }

    pub(crate) fn ensure_playback_controller(&self, can_gc: CanGc) -> Fallible<()> {
        let Some(element) = self.attached_element.get() else {
            return Ok(());
        };

        let video_id = if let Some(video_id) = self.attached_video_id.get() {
            video_id
        } else {
            if self.source_buffers.borrow().is_empty() {
                return Ok(());
            }
            let video_id = element.create_mse_media_player(self)?;
            self.attached_video_id.set(Some(video_id));
            if self.is_ended() {
                self.set_ready_state(READY_STATE_OPEN, can_gc);
            }
            video_id
        };

        for source_buffer in self.source_buffers.borrow().iter() {
            source_buffer.register_playback_input(video_id);
        }
        Ok(())
    }

    pub(crate) fn handle_media_event(&self, event: &MediaEvent, can_gc: CanGc) {
        match event {
            MediaEvent::MseAppendDone {
                input_id,
                input_buffered_ranges,
                buffered_ranges,
            } => {
                if let Some(source_buffer) = self.source_buffer_by_input_id(*input_id) {
                    source_buffer.notify_update_success(input_buffered_ranges, can_gc);
                }
                self.update_duration_from_buffered_ranges(buffered_ranges);
            }
            MediaEvent::MseInitSegmentParsed {
                input_id,
                duration_ms,
                video_tracks,
                audio_tracks,
                ..
            } => {
                self.update_source_buffer_track_metadata(*input_id, video_tracks, audio_tracks, can_gc);
                if !self.explicit_duration.get() && *duration_ms != 0 {
                    self.set_duration_value(*duration_ms as f64 / 1000.0, false);
                }
            }
            MediaEvent::MseError { input_id, .. } => {
                if let Some(source_buffer) = self.source_buffer_by_input_id(*input_id) {
                    source_buffer.notify_update_error(can_gc);
                }
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
        self.require_open("duration update")?;
        if duration.is_nan() || duration < 0.0 {
            return Err(Error::InvalidAccess(Some(
                "MediaSource duration must be non-negative".into(),
            )));
        }
        self.require_no_updating_source_buffers("duration update")?;
        self.set_duration_value(duration, true);
        Ok(())
    }

    fn AddSourceBuffer(&self, type_: DOMString) -> Fallible<DomRoot<SourceBuffer>> {
        self.require_open("addSourceBuffer")?;
        if !Self::is_type_supported_impl(&type_.str()) {
            return Err(Error::NotSupported(Some(
                "unsupported MediaSource MIME type".into(),
            )));
        }
        let source_buffer = SourceBuffer::new(
            &self.global(),
            None,
            self,
            type_.clone(),
            self.allocate_input_id(),
            CanGc::note(),
        );
        self.source_buffers
            .borrow_mut()
            .push(Dom::from_ref(&*source_buffer));
        self.sync_source_buffer_lists(CanGc::note());
        self.ensure_playback_controller(CanGc::note())?;
        Ok(source_buffer)
    }

    fn RemoveSourceBuffer(&self, source_buffer: &SourceBuffer) -> ErrorResult {
        self.require_no_updating_source_buffers("removeSourceBuffer")?;

        let removed = {
            let mut source_buffers = self.source_buffers.borrow_mut();
            source_buffers
                .iter()
                .position(|existing| std::ptr::eq(&**existing, source_buffer))
                .map(|index| source_buffers.remove(index))
        };

        let Some(removed) = removed else {
            return Err(Error::NotFound(None));
        };

        let removed = DomRoot::from_ref(&*removed);
        self.track_metadata_by_input
            .borrow_mut()
            .remove(&removed.input_id());
        if let Some(video_id) = self.attached_video_id.get() {
            removed.unregister_playback_input(video_id);
        }
        removed.mark_removed();
        removed.clear_for_detach();
        if self.source_buffers.borrow().is_empty() {
            if let Some(element) = self.attached_element.get() {
                element.reset_media_player();
            }
            self.attached_video_id.set(None);
        }
        self.sync_source_buffer_lists(CanGc::note());
        Ok(())
    }

    fn EndOfStream(&self) -> ErrorResult {
        self.require_open("endOfStream")?;
        self.require_no_updating_source_buffers("endOfStream")?;
        if let Some(video_id) = self.attached_video_id.get() {
            controller::end_mse_stream(video_id);
        }
        self.set_ready_state(READY_STATE_ENDED, CanGc::note());
        self.fire_simple_event("sourceended", CanGc::note());
        Ok(())
    }

    event_handler!(sourceopen, GetOnsourceopen, SetOnsourceopen);
    event_handler!(sourceended, GetOnsourceended, SetOnsourceended);
    event_handler!(sourceclose, GetOnsourceclose, SetOnsourceclose);
}
