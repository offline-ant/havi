/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use js::rust::HandleObject;
use stylo_atoms::Atom;

use crate::script::dom::bindings::codegen::GenericBindings::EventBinding::Event_Binding::EventMethods;
use crate::script::dom::bindings::codegen::Bindings::XRReferenceSpaceEventBinding::{
    XRReferenceSpaceEventInit, XRReferenceSpaceEventMethods,
};
use crate::script::dom::bindings::error::Fallible;
use crate::script::dom::bindings::inheritance::Castable;
use crate::script::dom::bindings::reflector::reflect_dom_object_with_proto;
use crate::script::dom::bindings::root::{Dom, DomRoot};
use crate::script::dom::bindings::str::DOMString;
use crate::script::dom::event::Event;
use crate::script::dom::window::Window;
use crate::script::dom::xrreferencespace::XRReferenceSpace;
use crate::script::dom::xrrigidtransform::XRRigidTransform;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct XRReferenceSpaceEvent {
    event: Event,
    space: Dom<XRReferenceSpace>,
    transform: Option<Dom<XRRigidTransform>>,
}

impl XRReferenceSpaceEvent {
    fn new_inherited(
        space: &XRReferenceSpace,
        transform: Option<&XRRigidTransform>,
    ) -> XRReferenceSpaceEvent {
        XRReferenceSpaceEvent {
            event: Event::new_inherited(),
            space: Dom::from_ref(space),
            transform: transform.map(Dom::from_ref),
        }
    }

    pub(crate) fn new(
        window: &Window,
        type_: Atom,
        bubbles: bool,
        cancelable: bool,
        space: &XRReferenceSpace,
        transform: Option<&XRRigidTransform>,
        can_gc: CanGc,
    ) -> DomRoot<XRReferenceSpaceEvent> {
        Self::new_with_proto(
            window, None, type_, bubbles, cancelable, space, transform, can_gc,
        )
    }

    #[expect(clippy::too_many_arguments)]
    fn new_with_proto(
        window: &Window,
        proto: Option<HandleObject>,
        type_: Atom,
        bubbles: bool,
        cancelable: bool,
        space: &XRReferenceSpace,
        transform: Option<&XRRigidTransform>,
        can_gc: CanGc,
    ) -> DomRoot<XRReferenceSpaceEvent> {
        let trackevent = reflect_dom_object_with_proto(
            Box::new(XRReferenceSpaceEvent::new_inherited(space, transform)),
            window,
            proto,
            can_gc,
        );
        {
            let event = trackevent.upcast::<Event>();
            event.init_event(type_, bubbles, cancelable);
        }
        trackevent
    }
}

impl XRReferenceSpaceEventMethods<crate::DomTypeHolder> for XRReferenceSpaceEvent {
    /// <https://www.w3.org/TR/webxr/#dom-xrreferencespaceevent-xrreferencespaceevent>
    fn Constructor(
        window: &Window,
        proto: Option<HandleObject>,
        can_gc: CanGc,
        type_: DOMString,
        init: &XRReferenceSpaceEventInit,
    ) -> Fallible<DomRoot<XRReferenceSpaceEvent>> {
        Ok(XRReferenceSpaceEvent::new_with_proto(
            window,
            proto,
            Atom::from(type_),
            init.parent.bubbles,
            init.parent.cancelable,
            &init.referenceSpace,
            init.transform.as_deref(),
            can_gc,
        ))
    }

    /// <https://www.w3.org/TR/webxr/#dom-xrreferencespaceeventinit-session>
    fn ReferenceSpace(&self) -> DomRoot<XRReferenceSpace> {
        DomRoot::from_ref(&*self.space)
    }

    /// <https://www.w3.org/TR/webxr/#dom-xrreferencespaceevent-transform>
    fn GetTransform(&self) -> Option<DomRoot<XRRigidTransform>> {
        self.transform
            .as_ref()
            .map(|transform| DomRoot::from_ref(&**transform))
    }

    /// <https://dom.spec.whatwg.org/#dom-event-istrusted>
    fn IsTrusted(&self) -> bool {
        self.event.IsTrusted()
    }
}
