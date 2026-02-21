/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR Result DOM binding.
//!
//! Wraps HPPR responses with optional response envelope metadata.

use dom_struct::dom_struct;
use hppr_client::{HpprResponse, ResponseKind};
use js::jsapi::Heap;
use js::jsval::{JSVal, UndefinedValue};
use js::rust::MutableHandleValue;
use script_bindings::conversions::SafeToJSValConvertible;

use crate::dom::bindings::codegen::Bindings::HpprClientBinding::HpprGreeting;
use crate::dom::bindings::codegen::Bindings::HpprResultBinding::HpprResultMethods;
use crate::dom::bindings::error::Error;
use crate::dom::bindings::reflector::{reflect_dom_object, Reflector};
use crate::dom::bindings::root::DomRoot;
use crate::dom::bindings::str::DOMString;
use crate::dom::globalscope::GlobalScope;
use crate::dom::hpprpacket::HpprPacket;
use crate::realms::enter_realm;
use crate::script_runtime::{CanGc, JSContext};
use script_bindings::cformat;

#[dom_struct]
pub(crate) struct HpprResult {
    reflector_: Reflector,
    #[ignore_malloc_size_of = "mozjs"]
    value: Heap<JSVal>,
}

impl HpprResult {
    fn new_inherited() -> Self {
        Self {
            reflector_: Reflector::new(),
            value: Heap::default(),
        }
    }

    fn lines_from_text(text: &str) -> Vec<DOMString> {
        text.lines()
            .filter(|line| !line.is_empty())
            .map(DOMString::from)
            .collect()
    }

    fn value_from_kind(
        global: &GlobalScope,
        kind: ResponseKind,
        can_gc: CanGc,
    ) -> Result<JSVal, Error> {
        let cx = GlobalScope::get_cx();
        let _ac = enter_realm(global);
        rooted!(in(*cx) let mut rval = UndefinedValue());

        match kind {
            ResponseKind::Empty => {
                // leave as undefined
            },
            ResponseKind::Greeting(ref greeting) => {
                let dict = HpprGreeting {
                    repoName: DOMString::from(greeting.repo_name()),
                    sessionId: DOMString::from(greeting.session_id()),
                    verifyingKey: DOMString::from(greeting.verifying_key()),
                    phc: Some(greeting.phc().map(|s| DOMString::from(s.to_string()))),
                    format: DOMString::from(greeting.format()),
                    commands: greeting
                        .commands()
                        .iter()
                        .map(|c| DOMString::from(format!("{} {}", c.name, c.version)))
                        .collect(),
                    status: Some(greeting.status().map(|s| DOMString::from(s.to_string()))),
                    uptime: Some(greeting.uptime().map(|s| DOMString::from(s.to_string()))),
                    version: Some(
                        greeting
                            .hpprd_version()
                            .map(|s| DOMString::from(s.to_string())),
                    ),
                    backend: Some(
                        greeting
                            .hpprd_backend()
                            .map(|s| DOMString::from(s.to_string())),
                    ),
                };
                dict.safe_to_jsval(cx, rval.handle_mut(), can_gc);
            },
            ResponseKind::Lines(text) => {
                let lines = Self::lines_from_text(&text);
                lines.safe_to_jsval(cx, rval.handle_mut(), can_gc);
            },

            ResponseKind::Packet(packet) => {
                let packet_dom =
                    HpprPacket::new(global, packet, can_gc).map_err(|e| Error::Type(cformat!("{}", e)))?;
                (&*packet_dom).safe_to_jsval(cx, rval.handle_mut(), can_gc);
            },
            ResponseKind::Exchange(result) => {
                let text = format!("received {}", result.received.len());
                let dom_string = DOMString::from(text);
                dom_string.safe_to_jsval(cx, rval.handle_mut(), can_gc);
            },
        }

        Ok(rval.get())
    }

    pub(crate) fn value_from_response(
        global: &GlobalScope,
        response: HpprResponse,
        can_gc: CanGc,
    ) -> Result<JSVal, Error> {
        let HpprResponse { kind, .. } = response;
        Self::value_from_kind(global, kind, can_gc)
    }

    pub(crate) fn new(
        global: &GlobalScope,
        response: HpprResponse,
        can_gc: CanGc,
    ) -> Result<DomRoot<Self>, Error> {
        let HpprResponse { kind } = response;

        let result = reflect_dom_object(
            Box::new(Self::new_inherited()),
            global,
            can_gc,
        );
        let value = Self::value_from_kind(global, kind, can_gc)?;
        result.value.set(value);
        Ok(result)
    }
}

impl HpprResultMethods<crate::DomTypeHolder> for HpprResult {
    fn Value(&self, _cx: JSContext, mut retval: MutableHandleValue) {
        retval.set(self.value.get())
    }

    fn GetResponseEnvelope(&self) -> Option<DomRoot<HpprPacket>> {
        None
    }
}
