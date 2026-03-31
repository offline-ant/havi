/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use js::rust::HandleObject;

use crate::script::dom::bindings::codegen::GenericBindings::WebGPUBinding::GPUInternalError_Binding::GPUInternalErrorMethods;
use crate::script::dom::bindings::reflector::reflect_dom_object_with_proto;
use crate::script::dom::bindings::root::DomRoot;
use crate::script::dom::bindings::str::DOMString;
use crate::script::dom::globalscope::GlobalScope;
use crate::script::dom::types::GPUError;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct GPUInternalError {
    gpu_error: GPUError,
}

impl GPUInternalError {
    fn new_inherited(message: DOMString) -> Self {
        Self {
            gpu_error: GPUError::new_inherited(message),
        }
    }

    pub(crate) fn new_with_proto(
        global: &GlobalScope,
        proto: Option<HandleObject>,
        message: DOMString,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object_with_proto(
            Box::new(Self::new_inherited(message)),
            global,
            proto,
            can_gc,
        )
    }
}

impl GPUInternalErrorMethods<crate::DomTypeHolder> for GPUInternalError {
    /// <https://gpuweb.github.io/gpuweb/#dom-GPUInternalError-GPUInternalError>
    fn Constructor(
        global: &GlobalScope,
        proto: Option<HandleObject>,
        can_gc: CanGc,
        message: DOMString,
    ) -> DomRoot<Self> {
        Self::new_with_proto(global, proto, message, can_gc)
    }
}
