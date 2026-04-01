/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;

use dom_struct::dom_struct;
use js::rust::HandleObject;
use stylo_atoms::Atom;

use crate::dom::bindings::cell::DomRefCell;
use crate::dom::bindings::codegen::Bindings::SourceBufferBinding::SourceBufferMethods;
use crate::dom::bindings::codegen::UnionTypes::ArrayBufferViewOrArrayBuffer;
use crate::dom::bindings::error::{Error, ErrorResult};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::num::Finite;
use crate::dom::bindings::reflector::{DomGlobal, reflect_dom_object_with_proto};
use crate::dom::bindings::root::{Dom, DomRoot};
use crate::dom::bindings::str::DOMString;
use crate::dom::eventtarget::EventTarget;
use crate::dom::globalscope::GlobalScope;
use crate::dom::media::mediasource::MediaSource;
use crate::dom::timeranges::{TimeRanges, TimeRangesContainer};
use crate::script_runtime::CanGc;
use media::controller::{self, MseSourceBufferInputId};

const STATE_ATTACHED: u8 = 0;
const STATE_REMOVED: u8 = 1;

const OPERATION_NONE: u8 = 0;
const OPERATION_APPEND: u8 = 1;
const OPERATION_REMOVE: u8 = 2;

#[dom_struct]
pub(crate) struct SourceBuffer {
    eventtarget: EventTarget,
    media_source: Dom<MediaSource>,
    mime_type: DomRefCell<DOMString>,
    input_id: MseSourceBufferInputId,
    state: Cell<u8>,
    pending_operation: Cell<u8>,
    updating: Cell<bool>,
    buffered: DomRefCell<TimeRangesContainer>,
}

impl SourceBuffer {
    fn new_inherited(
        media_source: &MediaSource,
        mime_type: DOMString,
        input_id: MseSourceBufferInputId,
    ) -> Self {
        Self {
            eventtarget: EventTarget::new_inherited(),
            media_source: Dom::from_ref(media_source),
            mime_type: DomRefCell::new(mime_type),
            input_id,
            state: Cell::new(STATE_ATTACHED),
            pending_operation: Cell::new(OPERATION_NONE),
            updating: Cell::new(false),
            buffered: DomRefCell::new(TimeRangesContainer::default()),
        }
    }

    pub(crate) fn new(
        global: &GlobalScope,
        proto: Option<HandleObject>,
        media_source: &MediaSource,
        mime_type: DOMString,
        input_id: MseSourceBufferInputId,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object_with_proto(
            Box::new(Self::new_inherited(media_source, mime_type, input_id)),
            global,
            proto,
            can_gc,
        )
    }

    pub(crate) fn mime_type(&self) -> DOMString {
        self.mime_type.borrow().clone()
    }

    pub(crate) fn input_id(&self) -> MseSourceBufferInputId {
        self.input_id
    }

    pub(crate) fn is_updating(&self) -> bool {
        self.updating.get()
    }

    pub(crate) fn mark_removed(&self) {
        self.state.set(STATE_REMOVED);
    }

    pub(crate) fn is_removed(&self) -> bool {
        self.state.get() == STATE_REMOVED
    }

    fn fire_simple_event(&self, name: &str, can_gc: CanGc) {
        self.upcast::<EventTarget>()
            .fire_event(Atom::from(name), can_gc);
    }

    fn set_buffered_ranges(&self, buffered_ranges: &[(f64, f64)]) {
        let mut buffered = TimeRangesContainer::default();
        for (start, end) in buffered_ranges {
            let _ = buffered.add(*start, *end);
        }
        *self.buffered.borrow_mut() = buffered;
    }

    pub(crate) fn notify_update_success(&self, buffered_ranges: &[(f64, f64)], can_gc: CanGc) {
        self.set_buffered_ranges(buffered_ranges);
        if self.updating.replace(false) {
            self.pending_operation.set(OPERATION_NONE);
            self.fire_simple_event("update", can_gc);
            self.fire_simple_event("updateend", can_gc);
        }
    }

    pub(crate) fn notify_update_error(&self, can_gc: CanGc) {
        if self.updating.replace(false) {
            self.pending_operation.set(OPERATION_NONE);
            self.fire_simple_event("error", can_gc);
            self.fire_simple_event("updateend", can_gc);
        }
    }

    pub(crate) fn clear_for_detach(&self) {
        self.updating.set(false);
        self.pending_operation.set(OPERATION_NONE);
        *self.buffered.borrow_mut() = TimeRangesContainer::default();
    }

    fn begin_update(&self, operation: u8, can_gc: CanGc) {
        self.pending_operation.set(operation);
        self.updating.set(true);
        self.fire_simple_event("updatestart", can_gc);
    }

    fn require_attached(&self, operation: &str) -> ErrorResult {
        if self.is_removed() || !self.media_source.has_source_buffer(self) {
            return Err(Error::InvalidState(Some(format!(
                "SourceBuffer has been removed during {operation}"
            ))));
        }
        Ok(())
    }

    fn require_not_updating(&self, operation: &str) -> ErrorResult {
        if self.updating.get() {
            return Err(Error::InvalidState(Some(format!(
                "SourceBuffer is already updating during {operation}"
            ))));
        }
        Ok(())
    }

    fn require_valid_remove_range(&self, start: f64, end: f64) -> ErrorResult {
        if start < 0.0 || end <= start {
            return Err(Error::InvalidAccess(Some(
                "SourceBuffer remove range must satisfy 0 <= start < end".into(),
            )));
        }
        Ok(())
    }

    fn require_active_video_id(&self, can_gc: CanGc, operation: &str) -> Result<u64, Error> {
        self.require_attached(operation)?;
        self.media_source.ensure_playback_controller(can_gc)?;
        self.media_source.attached_video_id().ok_or_else(|| {
            Error::InvalidState(Some(format!(
                "MediaSource is not attached during {operation}"
            )))
        })
    }

    pub(crate) fn register_playback_input(&self, video_id: u64) {
        controller::add_mse_source_buffer(video_id, self.input_id, self.mime_type().str().to_string());
    }

    pub(crate) fn unregister_playback_input(&self, video_id: u64) {
        controller::remove_mse_source_buffer(video_id, self.input_id);
    }
}

impl SourceBufferMethods<crate::DomTypeHolder> for SourceBuffer {
    fn Updating(&self) -> bool {
        self.updating.get()
    }

    fn Buffered(&self) -> DomRoot<TimeRanges> {
        TimeRanges::new(&self.global().as_window(), self.buffered.borrow().clone(), CanGc::note())
    }

    fn AppendBuffer(&self, data: ArrayBufferViewOrArrayBuffer) -> ErrorResult {
        self.require_not_updating("appendBuffer")?;
        let video_id = self.require_active_video_id(CanGc::note(), "appendBuffer")?;
        let bytes = match data {
            ArrayBufferViewOrArrayBuffer::ArrayBufferView(view) => view.to_vec(),
            ArrayBufferViewOrArrayBuffer::ArrayBuffer(buffer) => buffer.to_vec(),
        };
        self.begin_update(OPERATION_APPEND, CanGc::note());
        controller::append_mse_data(video_id, self.input_id, bytes);
        Ok(())
    }

    fn Remove(&self, start: Finite<f64>, end: Finite<f64>) -> ErrorResult {
        self.require_not_updating("remove")?;
        self.require_valid_remove_range(*start, *end)?;
        let video_id = self.require_active_video_id(CanGc::note(), "remove")?;
        self.begin_update(OPERATION_REMOVE, CanGc::note());
        controller::remove_mse_data(video_id, self.input_id, *start, *end);
        Ok(())
    }

    fn Abort(&self) -> ErrorResult {
        self.require_attached("abort")?;
        self.updating.set(false);
        self.pending_operation.set(OPERATION_NONE);
        Ok(())
    }

    event_handler!(updatestart, GetOnupdatestart, SetOnupdatestart);
    event_handler!(update, GetOnupdate, SetOnupdate);
    event_handler!(updateend, GetOnupdateend, SetOnupdateend);
    event_handler!(error, GetOnerror, SetOnerror);
}
