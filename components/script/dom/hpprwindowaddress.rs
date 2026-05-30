/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use hppr_packet::urc::URC as HpprURC;
use jsonqa::Qa;
use servo_url::BrowserUrl;

use crate::dom::bindings::codegen::Bindings::HpprWindowAddressBinding::HpprWindowAddressMethods;
use crate::dom::bindings::codegen::Bindings::URCBinding::URCMethods;
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::{DomGlobal, reflect_dom_object};
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::bindings::str::DOMString;
use crate::dom::globalscope::GlobalScope;
use crate::dom::urc::URC;
use crate::dom::window::Window;
use crate::dom::windowaddress::{WindowAddress, build_coordinate, format_optional_qa, parse_optional_qa};
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct HpprWindowAddress {
    windowaddress: WindowAddress,
    urc: MutNullableDom<URC>,
}

impl HpprWindowAddress {
    fn new_inherited(window: &Window) -> Self {
        Self {
            windowaddress: WindowAddress::new_inherited(window),
            urc: MutNullableDom::new(None),
        }
    }

    pub(crate) fn new(window: &Window, can_gc: CanGc) -> DomRoot<Self> {
        reflect_dom_object(Box::new(Self::new_inherited(window)), window.upcast::<GlobalScope>(), can_gc)
    }

    pub(crate) fn current_urc_state(&self) -> (HpprURC, Option<Qa>) {
        match self.upcast::<WindowAddress>().current_url() {
            BrowserUrl::Hppr(data) => (data.address().urc().clone(), parse_optional_qa(data.jsonqa())),
            _ => panic!("HpprWindowAddress used outside an HPPR-family document"),
        }
    }

    pub(crate) fn set_live_qa(&self, qa: Option<Qa>) -> Fallible<()> {
        let url = self.upcast::<WindowAddress>().current_url();
        let BrowserUrl::Hppr(data) = url else {
            return Err(Error::InvalidState(None));
        };
        let href = format!(
            "{}:{}{}",
            data.address().scheme().prefix().trim_end_matches(':'),
            data.address().urc_string(),
            format_optional_qa(qa.as_ref()),
        );
        self.upcast::<WindowAddress>()
            .navigate_to_href(&href, CanGc::note())
    }

    fn set_live_urc(&self, urc: &HpprURC, qa: Option<&Qa>) -> ErrorResult {
        let url = self.upcast::<WindowAddress>().current_url();
        let BrowserUrl::Hppr(data) = url else {
            return Err(Error::InvalidState(None));
        };
        let href = format!(
            "{}:{}{}",
            data.address().scheme().prefix().trim_end_matches(':'),
            urc,
            format_optional_qa(qa),
        );
        self.upcast::<WindowAddress>()
            .navigate_to_href(&href, CanGc::note())
    }
}

impl HpprWindowAddressMethods<crate::DomTypeHolder> for HpprWindowAddress {
    fn GetCoordinate(&self) -> Option<DOMString> {
        self.Urc().GetCoordinate()
    }

    fn Urc(&self) -> DomRoot<URC> {
        self.urc
            .or_init(|| URC::new_live(&self.global(), self, CanGc::note()))
    }

    fn GetGroup(&self) -> Fallible<Option<DOMString>> {
        Ok(self.Urc().GetGroup())
    }

    fn SetGroup(&self, value: Option<DOMString>) -> ErrorResult {
        let group = value.map(|v| v.to_string()).unwrap_or_default();
        let (current, qa) = self.current_urc_state();
        let parts = current.parts();
        let coord = build_coordinate(&group, &parts.api, &parts.key, current.is_listing());
        let next = HpprURC::parse(coord).map_err(|e| Error::Syntax(Some(e.to_string())))?;
        self.set_live_urc(&next, qa.as_ref())
    }

    fn GetApi(&self) -> Fallible<Option<DOMString>> {
        Ok(self.Urc().GetApi())
    }

    fn SetApi(&self, value: Option<DOMString>) -> ErrorResult {
        let api = value.map(|v| v.to_string()).unwrap_or_default();
        let (current, qa) = self.current_urc_state();
        let parts = current.parts();
        let coord = build_coordinate(&parts.group, &api, &parts.key, current.is_listing());
        let next = HpprURC::parse(coord).map_err(|e| Error::Syntax(Some(e.to_string())))?;
        self.set_live_urc(&next, qa.as_ref())
    }

    fn GetKey(&self) -> Fallible<Option<DOMString>> {
        Ok(self.Urc().GetKey())
    }

    fn SetKey(&self, value: Option<DOMString>) -> ErrorResult {
        let key = value.map(|v| v.to_string()).unwrap_or_default();
        let (current, qa) = self.current_urc_state();
        let parts = current.parts();
        let coord = build_coordinate(&parts.group, &parts.api, &key, current.is_listing());
        let next = HpprURC::parse(coord).map_err(|e| Error::Syntax(Some(e.to_string())))?;
        self.set_live_urc(&next, qa.as_ref())
    }
}
