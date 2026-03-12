/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;

use crate::dom::bindings::cell::DomRefCell;
use crate::dom::bindings::codegen::Bindings::SourceBufferListBinding::SourceBufferListMethods;
use crate::dom::bindings::reflector::reflect_dom_object;
use crate::dom::bindings::root::{Dom, DomRoot};
use crate::dom::eventtarget::EventTarget;
use crate::dom::media::sourcebuffer::SourceBuffer;
use crate::dom::window::Window;
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct SourceBufferList {
    eventtarget: EventTarget,
    source_buffers: DomRefCell<Vec<Dom<SourceBuffer>>>,
}

impl SourceBufferList {
    fn new_inherited(source_buffers: &[&SourceBuffer]) -> Self {
        Self {
            eventtarget: EventTarget::new_inherited(),
            source_buffers: DomRefCell::new(
                source_buffers.iter().map(|source_buffer| Dom::from_ref(&**source_buffer)).collect(),
            ),
        }
    }

    pub(crate) fn new(
        window: &Window,
        source_buffers: &[&SourceBuffer],
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self::new_inherited(source_buffers)),
            window,
            can_gc,
        )
    }

    pub(crate) fn replace_all(&self, source_buffers: Vec<Dom<SourceBuffer>>) {
        *self.source_buffers.borrow_mut() = source_buffers;
    }

    fn item(&self, index: usize) -> Option<DomRoot<SourceBuffer>> {
        self.source_buffers
            .borrow()
            .get(index)
            .map(|source_buffer| DomRoot::from_ref(&**source_buffer))
    }
}

impl SourceBufferListMethods<crate::DomTypeHolder> for SourceBufferList {
    fn Length(&self) -> u32 {
        self.source_buffers.borrow().len() as u32
    }

    fn IndexedGetter(&self, index: u32) -> Option<DomRoot<SourceBuffer>> {
        self.item(index as usize)
    }
}
