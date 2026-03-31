/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use js::jsapi::Heap;
use js::jsval::JSVal;
use js::rust::{HandleObject, HandleValue, MutableHandleValue};
use stylo_atoms::Atom;

use crate::script::dom::bindings::codegen::GenericBindings::EventBinding::EventMethods;
use crate::script::dom::bindings::codegen::Bindings::PopStateEventBinding;
use crate::script::dom::bindings::codegen::GenericBindings::PopStateEventBinding::PopStateEventMethods;
use crate::script::dom::bindings::error::Fallible;
use crate::script::dom::bindings::inheritance::Castable;
use crate::script::dom::bindings::reflector::reflect_dom_object_with_proto;
use crate::script::dom::bindings::root::DomRoot;
use crate::script::dom::bindings::str::DOMString;
use crate::script::dom::bindings::trace::RootedTraceableBox;
use crate::script::dom::event::Event;
use crate::script::dom::eventtarget::EventTarget;
use crate::script::dom::window::Window;
use crate::script::script_runtime::{CanGc, JSContext};

// https://html.spec.whatwg.org/multipage/#the-popstateevent-interface
#[dom_struct]
pub(crate) struct PopStateEvent {
    event: Event,
    #[ignore_malloc_size_of = "Defined in rust-mozjs"]
    state: Heap<JSVal>,
}

impl PopStateEvent {
    fn new_inherited() -> PopStateEvent {
        PopStateEvent {
            event: Event::new_inherited(),
            state: Heap::default(),
        }
    }

    fn new_uninitialized(
        window: &Window,
        proto: Option<HandleObject>,
        can_gc: CanGc,
    ) -> DomRoot<PopStateEvent> {
        reflect_dom_object_with_proto(
            Box::new(PopStateEvent::new_inherited()),
            window,
            proto,
            can_gc,
        )
    }

    fn new(
        window: &Window,
        proto: Option<HandleObject>,
        type_: Atom,
        bubbles: bool,
        cancelable: bool,
        state: HandleValue,
        can_gc: CanGc,
    ) -> DomRoot<PopStateEvent> {
        let ev = PopStateEvent::new_uninitialized(window, proto, can_gc);
        ev.state.set(state.get());
        {
            let event = ev.upcast::<Event>();
            event.init_event(type_, bubbles, cancelable);
        }
        ev
    }

    pub(crate) fn dispatch_jsval(
        target: &EventTarget,
        window: &Window,
        state: HandleValue,
        can_gc: CanGc,
    ) {
        let event =
            PopStateEvent::new(window, None, atom!("popstate"), false, false, state, can_gc);
        event.upcast::<Event>().fire(target, can_gc);
    }
}

impl PopStateEventMethods<crate::DomTypeHolder> for PopStateEvent {
    /// <https://html.spec.whatwg.org/multipage/#popstateevent>
    fn Constructor(
        window: &Window,
        proto: Option<HandleObject>,
        can_gc: CanGc,
        type_: DOMString,
        init: RootedTraceableBox<PopStateEventBinding::PopStateEventInit>,
    ) -> Fallible<DomRoot<PopStateEvent>> {
        Ok(PopStateEvent::new(
            window,
            proto,
            Atom::from(type_),
            init.parent.bubbles,
            init.parent.cancelable,
            init.state.handle(),
            can_gc,
        ))
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-popstateevent-state>
    fn State(&self, _cx: JSContext, mut retval: MutableHandleValue) {
        retval.set(self.state.get())
    }

    /// <https://dom.spec.whatwg.org/#dom-event-istrusted>
    fn IsTrusted(&self) -> bool {
        self.event.IsTrusted()
    }
}
