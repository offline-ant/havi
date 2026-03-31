/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use js::rust::HandleObject;

use crate::script::dom::bindings::codegen::GenericBindings::XRMediaBindingBinding::XRMediaBinding_Binding::XRMediaBindingMethods;
use crate::script::dom::bindings::codegen::Bindings::XRMediaBindingBinding::XRMediaLayerInit;
use crate::script::dom::bindings::error::{Error, Fallible};
use crate::script::dom::bindings::reflector::{Reflector, reflect_dom_object_with_proto};
use crate::script::dom::bindings::root::{Dom, DomRoot};
use crate::script::dom::html::htmlvideoelement::HTMLVideoElement;
use crate::script::dom::window::Window;
use crate::script::dom::xrcylinderlayer::XRCylinderLayer;
use crate::script::dom::xrequirectlayer::XREquirectLayer;
use crate::script::dom::xrquadlayer::XRQuadLayer;
use crate::script::dom::xrsession::XRSession;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct XRMediaBinding {
    reflector: Reflector,
    session: Dom<XRSession>,
}

impl XRMediaBinding {
    pub(crate) fn new_inherited(session: &XRSession) -> XRMediaBinding {
        XRMediaBinding {
            reflector: Reflector::new(),
            session: Dom::from_ref(session),
        }
    }

    fn new(
        global: &Window,
        proto: Option<HandleObject>,
        session: &XRSession,
        can_gc: CanGc,
    ) -> DomRoot<XRMediaBinding> {
        reflect_dom_object_with_proto(
            Box::new(XRMediaBinding::new_inherited(session)),
            global,
            proto,
            can_gc,
        )
    }
}

impl XRMediaBindingMethods<crate::DomTypeHolder> for XRMediaBinding {
    /// <https://immersive-web.github.io/layers/#dom-xrmediabinding-xrmediabinding>
    fn Constructor(
        global: &Window,
        proto: Option<HandleObject>,
        can_gc: CanGc,
        session: &XRSession,
    ) -> Fallible<DomRoot<XRMediaBinding>> {
        // Step 1.
        if session.is_ended() {
            return Err(Error::InvalidState(None));
        }

        // Step 2.
        if !session.is_immersive() {
            return Err(Error::InvalidState(None));
        }

        // Steps 3-5.
        Ok(XRMediaBinding::new(global, proto, session, can_gc))
    }

    /// <https://immersive-web.github.io/layers/#dom-xrmediabinding-createquadlayer>
    fn CreateQuadLayer(
        &self,
        _: &HTMLVideoElement,
        _: &XRMediaLayerInit,
    ) -> Fallible<DomRoot<XRQuadLayer>> {
        // https://github.com/servo/servo/issues/27493
        Err(Error::NotSupported(None))
    }

    /// <https://immersive-web.github.io/layers/#dom-xrmediabinding-createcylinderlayer>
    fn CreateCylinderLayer(
        &self,
        _: &HTMLVideoElement,
        _: &XRMediaLayerInit,
    ) -> Fallible<DomRoot<XRCylinderLayer>> {
        // https://github.com/servo/servo/issues/27493
        Err(Error::NotSupported(None))
    }

    /// <https://immersive-web.github.io/layers/#dom-xrmediabinding-createequirectlayer>
    fn CreateEquirectLayer(
        &self,
        _: &HTMLVideoElement,
        _: &XRMediaLayerInit,
    ) -> Fallible<DomRoot<XREquirectLayer>> {
        // https://github.com/servo/servo/issues/27493
        Err(Error::NotSupported(None))
    }
}
