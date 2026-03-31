/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;

use crate::script::dom::bindings::codegen::GenericBindings::XRJointPoseBinding::XRJointPoseMethods;
use crate::script::dom::bindings::num::Finite;
use crate::script::dom::bindings::reflector::reflect_dom_object;
use crate::script::dom::bindings::root::DomRoot;
use crate::script::dom::window::Window;
use crate::script::dom::xrpose::XRPose;
use crate::script::dom::xrrigidtransform::XRRigidTransform;
use crate::script::dom::xrsession::ApiRigidTransform;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct XRJointPose {
    pose: XRPose,
    radius: Option<f32>,
}

impl XRJointPose {
    fn new_inherited(transform: &XRRigidTransform, radius: Option<f32>) -> XRJointPose {
        XRJointPose {
            pose: XRPose::new_inherited(transform),
            radius,
        }
    }

    pub(crate) fn new(
        window: &Window,
        pose: ApiRigidTransform,
        radius: Option<f32>,
        can_gc: CanGc,
    ) -> DomRoot<XRJointPose> {
        let transform = XRRigidTransform::new(window, pose, can_gc);
        reflect_dom_object(
            Box::new(XRJointPose::new_inherited(&transform, radius)),
            window,
            can_gc,
        )
    }
}

impl XRJointPoseMethods<crate::DomTypeHolder> for XRJointPose {
    /// <https://immersive-web.github.io/webxr/#dom-XRJointPose-views>
    fn GetRadius(&self) -> Option<Finite<f32>> {
        self.radius.map(Finite::wrap)
    }
}
