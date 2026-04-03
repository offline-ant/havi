/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::RefCell;
use std::ptr::NonNull;

use dom_struct::dom_struct;
use hppr_packet::urc::URC as HpprURC;
use js::jsapi::JSObject;
use js::rust::HandleObject;
use jsonqa::Qa;
use servo_url::BrowserUrl;

use crate::dom::bindings::codegen::Bindings::AddressBinding::AddressMethods;
use crate::dom::bindings::codegen::Bindings::URCBinding::URCMethods;
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::{Reflector, reflect_dom_object_with_proto};
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::bindings::str::{DOMString, USVString};
use crate::dom::globalscope::GlobalScope;
use crate::dom::urc::URC;
use crate::dom::window::Window;
use crate::dom::windowaddress::{build_coordinate, parse_optional_qa};
use crate::script_runtime::{CanGc, JSContext as SafeJSContext};

fn parse_address(input: &str) -> Result<(String, HpprURC, Option<Qa>), Error> {
    let parsed = BrowserUrl::parse(input)
        .map_err(|e| Error::Syntax(Some(format!("Invalid address: {e}"))))?;
    match parsed {
        BrowserUrl::Hppr(data) => Ok((
            data.address().scheme().prefix().trim_end_matches(':').to_string(),
            data.address().urc().clone(),
            parse_optional_qa(data.jsonqa()),
        )),
        BrowserUrl::FileDocument(_) | BrowserUrl::Web(_) => {
            Err(Error::Syntax(Some("Address only parses HPPR-family exact addresses".to_string())))
        }
    }
}

fn format_href(scheme: &str, urc: &HpprURC, qa: Option<&Qa>) -> USVString {
    let mut href = format!("{scheme}:{urc}");
    if let Some(qa) = qa {
        href.push_str(&qa.to_string());
    }
    USVString(href)
}

#[dom_struct]
pub(crate) struct Address {
    reflector_: Reflector,
    scheme_name: RefCell<String>,
    urc: MutNullableDom<URC>,
}

impl Address {
    fn new_inherited(scheme_name: String) -> Self {
        Self {
            reflector_: Reflector::new(),
            scheme_name: RefCell::new(scheme_name),
            urc: MutNullableDom::new(None),
        }
    }

    fn current_urc(&self) -> DomRoot<URC> {
        self.urc.get().expect("Address must always have a URC")
    }

    fn replace_state(&self, scheme_name: String, inner: HpprURC, qa: Option<Qa>) {
        *self.scheme_name.borrow_mut() = scheme_name;
        self.current_urc().set_detached_state(inner, qa);
    }
}

impl AddressMethods<crate::DomTypeHolder> for Address {
    fn Constructor(
        window: &Window,
        proto: Option<HandleObject>,
        can_gc: CanGc,
        input: USVString,
    ) -> Fallible<DomRoot<Self>> {
        let (scheme_name, inner, qa) = parse_address(&input.0)?;
        let global = window.upcast::<GlobalScope>();
        let address = reflect_dom_object_with_proto(
            Box::new(Self::new_inherited(scheme_name)),
            global,
            proto,
            can_gc,
        );
        let urc = URC::new_with_qa(global, inner, qa, can_gc);
        address.urc.set(Some(&urc));
        Ok(address)
    }

    fn GetHref(&self) -> Fallible<USVString> {
        let urc = self.current_urc();
        Ok(format_href(
            &self.scheme_name.borrow(),
            &urc.current_inner(),
            urc.current_qa().as_ref(),
        ))
    }

    fn SetHref(&self, value: USVString) -> ErrorResult {
        let (scheme_name, inner, qa) = parse_address(&value.0)?;
        self.replace_state(scheme_name, inner, qa);
        Ok(())
    }

    fn Scheme(&self) -> DOMString {
        DOMString::from(self.scheme_name.borrow().as_str())
    }

    fn GetCoordinate(&self) -> Option<DOMString> {
        self.current_urc().GetCoordinate()
    }

    fn Urc(&self) -> DomRoot<URC> {
        self.current_urc()
    }

    fn GetGroup(&self) -> Fallible<Option<DOMString>> {
        Ok(self.current_urc().GetGroup())
    }

    fn SetGroup(&self, value: Option<DOMString>) -> ErrorResult {
        let group = value.map(|v| v.to_string()).unwrap_or_default();
        let urc = self.current_urc();
        let current = urc.current_inner();
        let qa = urc.current_qa();
        let parts = current.parts();
        let next = HpprURC::parse(build_coordinate(&group, &parts.app, &parts.location, current.is_listing()))
            .map_err(|e| Error::Syntax(Some(e.to_string())))?;
        urc.set_detached_state(next, qa);
        Ok(())
    }

    fn GetApp(&self) -> Fallible<Option<DOMString>> {
        Ok(self.current_urc().GetApp())
    }

    fn SetApp(&self, value: Option<DOMString>) -> ErrorResult {
        let app = value.map(|v| v.to_string()).unwrap_or_default();
        let urc = self.current_urc();
        let current = urc.current_inner();
        let qa = urc.current_qa();
        let parts = current.parts();
        let next = HpprURC::parse(build_coordinate(&parts.group, &app, &parts.location, current.is_listing()))
            .map_err(|e| Error::Syntax(Some(e.to_string())))?;
        urc.set_detached_state(next, qa);
        Ok(())
    }

    fn GetLocation(&self) -> Fallible<Option<DOMString>> {
        Ok(self.current_urc().GetLocation())
    }

    fn SetLocation(&self, value: Option<DOMString>) -> ErrorResult {
        let location = value.map(|v| v.to_string()).unwrap_or_default();
        let urc = self.current_urc();
        let current = urc.current_inner();
        let qa = urc.current_qa();
        let parts = current.parts();
        let next = HpprURC::parse(build_coordinate(&parts.group, &parts.app, &location, current.is_listing()))
            .map_err(|e| Error::Syntax(Some(e.to_string())))?;
        urc.set_detached_state(next, qa);
        Ok(())
    }

    fn IsListing(&self) -> bool {
        self.current_urc().IsListing()
    }

    fn GetQa(&self, cx: SafeJSContext) -> Fallible<Option<NonNull<JSObject>>> {
        self.current_urc().GetQa(cx)
    }

    fn SetQa(&self, cx: SafeJSContext, qa: *mut JSObject) -> Fallible<()> {
        self.current_urc().SetQa(cx, qa)
    }

    fn GetFragment(&self) -> Option<DOMString> {
        self.current_urc().GetFragment()
    }
}
