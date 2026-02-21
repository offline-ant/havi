/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR Error DOM binding - protocol errors with structured information.

use dom_struct::dom_struct;
use embedder_traits::HpprProtocolError;
use js::gc::HandleObject;
use script_bindings::root::DomRoot;
use script_bindings::script_runtime::CanGc;
use script_bindings::str::DOMString;

use crate::dom::bindings::codegen::Bindings::HpprErrorBinding::HpprErrorMethods;
use crate::dom::bindings::error::Error;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::{reflect_dom_object, reflect_dom_object_with_proto};
use crate::dom::types::{DOMException, GlobalScope};
use crate::dom::window::Window;

/// Maps HPPR error types to DOMException names.
fn error_type_to_dom_name(error_type: &str) -> &'static str {
    match error_type {
        "NOT_FOUND" => "NotFoundError",
        "FORBIDDEN" | "NOT_ALLOWED" => "SecurityError",
        "UNAUTHORIZED" => "SecurityError",
        "INVALID" => "SyntaxError",
        "CONNECTION" | "TIMEOUT" => "NetworkError",
        "PACKET" => "DataError",
        "TOO_LARGE" => "QuotaExceededError",
        "EXPIRED" => "TimeoutError",
        "HELLO_REQUIRED" => "InvalidStateError",
        "INTERNAL" => "OperationError",
        _ => "OperationError",
    }
}

/// HPPR protocol error with structured information.
#[dom_struct]
pub(crate) struct HpprError {
    dom_exception: DOMException,
    error_type: DOMString,
    detail: DOMString,
    fatal: bool,
}

impl HpprError {
    fn new_inherited(error_type: DOMString, detail: DOMString, fatal: bool) -> Self {
        let dom_name = error_type_to_dom_name(&error_type.to_string());
        Self {
            dom_exception: DOMException::new_inherited(
                detail.clone(),
                DOMString::from(dom_name),
            ),
            error_type,
            detail,
            fatal,
        }
    }

    /// Create a new HpprError from structured error info.
    pub(crate) fn new(
        global: &GlobalScope,
        error_type: DOMString,
        detail: DOMString,
        fatal: bool,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self::new_inherited(error_type, detail, fatal)),
            global,
            can_gc,
        )
    }

    /// Create from HpprProtocolError (from embedder_traits).
    pub(crate) fn from_protocol_error(
        global: &GlobalScope,
        error: &HpprProtocolError,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        Self::new(
            global,
            DOMString::from(&*error.error_type),
            DOMString::from(&*error.detail),
            error.fatal,
            can_gc,
        )
    }
}

impl HpprErrorMethods<crate::DomTypeHolder> for HpprError {
    fn Constructor(
        window: &Window,
        proto: Option<HandleObject>,
        can_gc: CanGc,
        error_type: DOMString,
        detail: DOMString,
    ) -> Result<DomRoot<Self>, Error> {
        let global = window.upcast::<GlobalScope>();
        Ok(reflect_dom_object_with_proto(
            Box::new(HpprError::new_inherited(error_type, detail, false)),
            global,
            proto,
            can_gc,
        ))
    }

    fn Type(&self) -> DOMString {
        self.error_type.clone()
    }

    fn Detail(&self) -> DOMString {
        self.detail.clone()
    }

    fn Fatal(&self) -> bool {
        self.fatal
    }
}
