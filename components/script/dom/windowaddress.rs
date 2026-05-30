/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::ptr::NonNull;

use constellation_traits::NavigationHistoryBehavior;
use dom_struct::dom_struct;
use js::jsapi::JSObject;
use jsonqa::Qa;
use servo_url::BrowserUrl;

use crate::dom::bindings::codegen::Bindings::LocationBinding::Location_Binding::LocationMethods;
use crate::dom::bindings::codegen::Bindings::WindowAddressBinding::WindowAddressMethods;
use crate::dom::bindings::codegen::Bindings::WindowBinding::Window_Binding::WindowMethods;
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::{Reflector, reflect_dom_object};
use crate::dom::bindings::root::{Dom, DomRoot};
use crate::dom::bindings::str::{DOMString, USVString};
use crate::dom::globalscope::GlobalScope;
use crate::dom::history::History;
use crate::dom::urc::qa_to_js_object;
use crate::dom::window::Window;
use crate::script_runtime::{CanGc, JSContext as SafeJSContext};

pub(crate) fn parse_optional_qa(text: &str) -> Option<Qa> {
    if text.is_empty() {
        None
    } else {
        Qa::parse(text).ok()
    }
}

pub(crate) fn format_optional_qa(qa: Option<&Qa>) -> String {
    qa.map(ToString::to_string).unwrap_or_default()
}

pub(crate) fn build_coordinate(group: &str, api: &str, key: &str, is_listing: bool) -> String {
    match (group.is_empty(), api.is_empty(), key.is_empty()) {
        (true, _, _) => "//".to_string(),
        (false, true, _) => format!("//{group}/"),
        (false, false, true) if is_listing => format!("//{group}/{api}//"),
        (false, false, true) => format!("//{group}/{api}/"),
        (false, false, false) if is_listing => format!("//{group}/{api}//{key}/"),
        (false, false, false) => format!("//{group}/{api}//{key}"),
    }
}

#[dom_struct]
pub(crate) struct WindowAddress {
    reflector_: Reflector,
    window: Dom<Window>,
}

impl WindowAddress {
    pub(crate) fn new_inherited(window: &Window) -> Self {
        Self {
            reflector_: Reflector::new(),
            window: Dom::from_ref(window),
        }
    }

    pub(crate) fn new(window: &Window, can_gc: CanGc) -> DomRoot<Self> {
        reflect_dom_object(Box::new(Self::new_inherited(window)), window.upcast::<GlobalScope>(), can_gc)
    }

    pub(crate) fn window(&self) -> &Window {
        &self.window
    }

    pub(crate) fn current_url(&self) -> BrowserUrl {
        let url = self.window.Document().url();
        if url.matches_about_blank()
            && let Ok(href) = self.window.Location().GetHref()
            && let Ok(parsed) = BrowserUrl::parse(&href.0)
        {
            return parsed;
        }
        url
    }

    pub(crate) fn current_qa(&self) -> Option<Qa> {
        match self.current_url() {
            BrowserUrl::Hppr(data) => parse_optional_qa(data.jsonqa()),
            BrowserUrl::FileDocument(data) => parse_optional_qa(data.jsonqa()),
            BrowserUrl::Web(_) => None,
        }
    }

    pub(crate) fn navigate_to(&self, url: BrowserUrl, can_gc: CanGc) {
        let window = self.window();
        let document = window.Document();

        let incumbent_global = GlobalScope::incumbent().expect("no incumbent global object");
        let mut load_data = incumbent_global
            .as_window()
            .load_data_for_document(url, window.pipeline_id());
        load_data.about_base_url = document.about_base_url();

        let history_handling = if !document.completely_loaded() {
            NavigationHistoryBehavior::Replace
        } else {
            NavigationHistoryBehavior::Auto
        };

        window.load_url(history_handling, false, load_data, can_gc);
    }

    pub(crate) fn navigate_to_href(&self, href: &str, can_gc: CanGc) -> ErrorResult {
        let url = BrowserUrl::parse(href)
            .map_err(|e| Error::Syntax(Some(format!("Invalid URL: {e}"))))?;
        let current = self.current_url();
        if History::can_have_url_rewritten(&current, &url) {
            self.window.Document().set_url(url);
            return Ok(());
        }
        self.navigate_to(url, can_gc);
        Ok(())
    }
}

impl WindowAddressMethods<crate::DomTypeHolder> for WindowAddress {
    fn GetHref(&self) -> Fallible<USVString> {
        Ok(USVString(self.current_url().to_string()))
    }

    fn SetHref(&self, value: USVString) -> ErrorResult {
        let url = BrowserUrl::parse(&value.0)
            .map_err(|e| Error::Syntax(Some(format!("Invalid URL: {e}"))))?;
        let current = self.current_url();
        if History::can_have_url_rewritten(&current, &url) {
            self.window.Document().set_url(url);
            return Ok(());
        }
        self.navigate_to(url, CanGc::note());
        Ok(())
    }

    fn Scheme(&self) -> DOMString {
        DOMString::from(self.current_url().scheme())
    }

    fn GetQa(&self, cx: SafeJSContext) -> Fallible<Option<NonNull<JSObject>>> {
        match self.current_qa() {
            Some(qa) => Ok(qa_to_js_object(cx, &qa)),
            None => Ok(None),
        }
    }

    fn GetFragment(&self) -> Option<DOMString> {
        if let Some(qa) = self.current_qa() {
            return qa.fragment().map(DOMString::from);
        }
        match self.current_url() {
            BrowserUrl::Web(url) => url.fragment().map(DOMString::from),
            BrowserUrl::Hppr(_) | BrowserUrl::FileDocument(_) => None,
        }
    }

    fn IsListing(&self) -> bool {
        match self.current_url() {
            BrowserUrl::Hppr(data) => data.address().is_listing(),
            BrowserUrl::FileDocument(data) => data.document_url().path().ends_with('/'),
            BrowserUrl::Web(url) => url.path().ends_with('/'),
        }
    }
}
