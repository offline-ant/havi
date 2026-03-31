/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use webxr_api::LayerId;

use crate::script::dom::bindings::root::Dom;
use crate::script::dom::eventtarget::EventTarget;
use crate::script::dom::xrframe::XRFrame;
use crate::script::dom::xrsession::XRSession;

#[dom_struct]
pub(crate) struct XRLayer {
    event_target: EventTarget,
    session: Dom<XRSession>,
    /// If none, the session is inline (the composition disabled flag is true).
    #[no_trace]
    layer_id: Option<LayerId>,
}

impl XRLayer {
    pub(crate) fn new_inherited(
        session: &XRSession,
        layer_id: Option<LayerId>,
    ) -> XRLayer {
        XRLayer {
            event_target: EventTarget::new_inherited(),
            session: Dom::from_ref(session),
            layer_id,
        }
    }

    pub(crate) fn layer_id(&self) -> Option<LayerId> {
        self.layer_id
    }

    pub(crate) fn session(&self) -> &XRSession {
        &self.session
    }

    pub(crate) fn begin_frame(&self, _frame: &XRFrame) -> Option<()> {
        // WebGL layer support removed; other layer types not yet implemented
        unimplemented!()
    }

    pub(crate) fn end_frame(&self, _frame: &XRFrame) -> Option<()> {
        // WebGL layer support removed; other layer types not yet implemented
        unimplemented!()
    }
}
