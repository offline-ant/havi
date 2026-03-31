/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
use dom_struct::dom_struct;

use crate::script::dom::bindings::cell::DomRefCell;
use crate::script::dom::bindings::codegen::GenericBindings::DOMRectListBinding::DOMRectListMethods;
use crate::script::dom::bindings::reflector::{Reflector, reflect_dom_object_with_proto};
use crate::script::dom::bindings::root::{Dom, DomRoot};
use crate::script::dom::domrect::DOMRect;
use crate::script::dom::window::Window;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct DOMRectList {
    reflector_: Reflector,
    rects: DomRefCell<Vec<Dom<DOMRect>>>,
}

impl DOMRectList {
    fn new_inherited(rects: Vec<DomRoot<DOMRect>>) -> DOMRectList {
        DOMRectList {
            reflector_: Reflector::new(),
            rects: DomRefCell::new(
                rects
                    .into_iter()
                    .map(|dom_root| dom_root.as_traced())
                    .collect(),
            ),
        }
    }

    pub(crate) fn new(
        window: &Window,
        rects: Vec<DomRoot<DOMRect>>,
        can_gc: CanGc,
    ) -> DomRoot<DOMRectList> {
        reflect_dom_object_with_proto(
            Box::new(DOMRectList::new_inherited(rects)),
            window,
            None,
            can_gc,
        )
    }

    pub(crate) fn first(&self) -> Option<DomRoot<DOMRect>> {
        self.rects.borrow().first().map(Dom::as_rooted)
    }
}

impl DOMRectListMethods<crate::DomTypeHolder> for DOMRectList {
    /// <https://drafts.fxtf.org/geometry/#DOMRectList>
    fn Item(&self, index: u32) -> Option<DomRoot<DOMRect>> {
        self.rects.borrow().get(index as usize).map(Dom::as_rooted)
    }

    /// <https://drafts.fxtf.org/geometry/#DOMRectList>
    fn IndexedGetter(&self, index: u32) -> Option<DomRoot<DOMRect>> {
        self.Item(index)
    }

    /// <https://drafts.fxtf.org/geometry/#DOMRectList>
    fn Length(&self) -> u32 {
        self.rects.borrow().len() as u32
    }
}
