/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Address (HPPR Address) DOM binding.
//!
//! Represents an HPPR Address combining scheme, optional endpoint, and URC.
//! Format: `scheme://group/app/location{via:host:port,qa}`

use std::ptr::NonNull;

use constellation_traits::NavigationHistoryBehavior;
use dom_struct::dom_struct;
use hppr_packet::urc::URC as HpprURC;
use js::jsapi::JSObject;
use js::rust::HandleObject;
use jsonqa::Qa;
use servo_url::BrowserUrl;

use crate::dom::bindings::codegen::Bindings::AddressBinding::AddressMethods;
use crate::dom::bindings::codegen::Bindings::WindowBinding::Window_Binding::WindowMethods;
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::{DomGlobal, Reflector, reflect_dom_object, reflect_dom_object_with_proto};
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::bindings::str::{DOMString, USVString};
use crate::dom::globalscope::GlobalScope;
use crate::dom::urc::URC;
use crate::dom::window::Window;
use crate::script_runtime::{CanGc, JSContext as SafeJSContext};

/// Default HPPR port.
const DEFAULT_PORT: u16 = 4777;

/// Decode percent-encoded bytes in a string.
/// HPPR coordinates do not use percent-encoding; this undoes encoding
/// applied by the `url` crate when it processes hppr: URLs.
fn percent_decode(input: &str) -> String {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (
                hex_val(bytes[i + 1]),
                hex_val(bytes[i + 2]),
            ) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// HPPR URL schemes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum HpprScheme {
    Hppr,
    HpprSetup,
    HpprSandbox,
    HpprBrowse,
    HpprEditor,
}

impl HpprScheme {
    fn as_str(&self) -> &'static str {
        match self {
            HpprScheme::Hppr => "hppr",
            HpprScheme::HpprSetup => "hppr-setup",
            HpprScheme::HpprSandbox => "hppr-sandbox",
            HpprScheme::HpprBrowse => "hppr-browse",
            HpprScheme::HpprEditor => "hppr-editor",
        }
    }

    fn requires_endpoint(&self) -> bool {
        matches!(self, HpprScheme::HpprSetup | HpprScheme::HpprSandbox)
    }

    fn forbids_endpoint(&self) -> bool {
        matches!(self, HpprScheme::HpprEditor)
    }

    fn is_hppr_browsable(&self) -> bool {
        matches!(self, HpprScheme::Hppr | HpprScheme::HpprBrowse)
    }
}

/// Parsed endpoint with host and port.
#[derive(Debug, Clone)]
struct Endpoint {
    host: String,
    port: u16,
}

impl Endpoint {
    fn from_str_with_default(s: &str, default_port: u16) -> Self {
        // Handle IPv6 bracket form: [::1]:port or [::1]
        if s.starts_with('[') {
            if let Some(close) = s.find(']') {
                let host = &s[1..close];
                let rest = &s[close + 1..];
                if let Some(port_str) = rest.strip_prefix(':') {
                    if let Ok(port) = port_str.parse::<u16>() {
                        return Self { host: host.to_string(), port };
                    }
                }
                return Self { host: host.to_string(), port: default_port };
            }
        }
        if let Some(colon_pos) = s.rfind(':') {
            let after = &s[colon_pos + 1..];
            if let Ok(port) = after.parse::<u16>() {
                return Self {
                    host: s[..colon_pos].to_string(),
                    port,
                };
            }
        }
        Self {
            host: s.to_string(),
            port: default_port,
        }
    }

}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

/// Extract `via` value from JSONqa string, returning (via_value, remaining_qa).
///
/// The JSONqa string starts with `{` and ends with `}`.
/// Keys are `key:value` pairs separated by `,`.
fn extract_via(jsonqa: &str) -> (Option<String>, String) {
    let inner = if jsonqa.starts_with('{') && jsonqa.ends_with('}') {
        &jsonqa[1..jsonqa.len() - 1]
    } else {
        return (None, jsonqa.to_string());
    };

    if inner.is_empty() {
        return (None, String::new());
    }

    let mut via_value = None;
    let mut remaining = Vec::new();

    for part in inner.split(',') {
        if let Some(val) = part.strip_prefix("via:") {
            via_value = Some(val.to_string());
        } else {
            remaining.push(part);
        }
    }

    let remaining_str = if remaining.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", remaining.join(","))
    };

    (via_value, remaining_str)
}

/// DOM representation of an HPPR Address.
#[dom_struct]
pub(crate) struct Address {
    reflector_: Reflector,
    /// Parsed scheme name.
    scheme_name: String,
    /// Whether this scheme supports hasDirectEndpoint (hppr, hppr-browse).
    is_hppr_browsable: bool,
    /// Parsed endpoint (host:port) extracted from {via:...}, or None.
    endpoint: Option<String>,
    /// Inner URC coordinate (without qa metadata).
    #[ignore_malloc_size_of = "hppr_packet::urc::URC"]
    #[no_trace]
    inner_urc: HpprURC,
    /// Cached URC DOM object for [SameObject]. Owns the mutable qa state.
    urc_cache: MutNullableDom<URC>,
}

/// Build coordinate string from group/app/location components.
fn build_coordinate(group: &str, app: &str, location: &str) -> String {
    if location.is_empty() {
        if app.is_empty() {
            format!("//{}/", group)
        } else {
            format!("//{}/{}/", group, app)
        }
    } else {
        format!("//{}/{}/{}", group, app, location)
    }
}

impl Address {
    fn new_inherited(
        scheme: HpprScheme,
        endpoint: Option<String>,
        inner_urc: HpprURC,
    ) -> Self {
        Self {
            reflector_: Reflector::new(),
            scheme_name: scheme.as_str().to_string(),
            is_hppr_browsable: scheme.is_hppr_browsable(),
            endpoint,
            inner_urc,
            urc_cache: MutNullableDom::new(None),
        }
    }

    /// Create an Address from a URL string (for window.address).
    pub(crate) fn new_from_url(global: &GlobalScope, url: &str, can_gc: CanGc) -> DomRoot<Self> {
        match Self::parse(url) {
            Ok((scheme, endpoint, inner_urc, qa)) => {
                let address = Self::new_inherited(scheme, endpoint, inner_urc);
                let result = reflect_dom_object(Box::new(address), global, can_gc);
                // Eagerly create the URC with qa so Href() can read it.
                result.urc_cache.or_init(|| {
                    URC::new_with_qa(global, result.inner_urc.clone(), qa, can_gc)
                });
                result
            }
            Err(_) => {
                // For non-HPPR URLs (about:blank, file:, etc.), create empty Address
                let inner_urc = HpprURC::parse("//".to_string()).unwrap_or_else(|_| {
                    HpprURC::parse("//invalid/url/".to_string()).unwrap()
                });
                let address = Self::new_inherited(HpprScheme::Hppr, None, inner_urc);
                reflect_dom_object(Box::new(address), global, can_gc)
            }
        }
    }

    /// Parse an HPPR Address string.
    ///
    /// All schemes use `scheme://coord{via:endpoint}` format.
    /// Endpoint is extracted from `{via:...}` in JSONqa suffix.
    fn parse(url: &str) -> Result<(HpprScheme, Option<String>, HpprURC, Option<Qa>), Error> {
        // Detect scheme (order matters - longer prefixes first)
        let (scheme, rest) = if let Some(r) = url.strip_prefix("hppr-editor:") {
            (HpprScheme::HpprEditor, r)
        } else if let Some(r) = url.strip_prefix("hppr-setup:") {
            (HpprScheme::HpprSetup, r)
        } else if let Some(r) = url.strip_prefix("hppr-sandbox:") {
            (HpprScheme::HpprSandbox, r)
        } else if let Some(r) = url.strip_prefix("hppr-browse:") {
            (HpprScheme::HpprBrowse, r)
        } else if let Some(r) = url.strip_prefix("hppr:") {
            (HpprScheme::Hppr, r)
        } else {
            let scheme = url.split(':').next().unwrap_or("").to_string();
            return Err(Error::Syntax(Some(format!("Unknown scheme: {}", scheme))));
        };

        // All schemes: rest must start with //
        if !rest.starts_with("//") {
            return Err(Error::Syntax(Some("Missing // coordinate marker".to_string())));
        }

        // The url crate percent-encodes characters like { } in paths.
        // HPPR coordinates do not use percent-encoding, so decode it.
        let urc_decoded;
        let rest = if rest.contains('%') {
            urc_decoded = percent_decode(rest);
            urc_decoded.as_str()
        } else {
            rest
        };

        // Split off JSONqa suffix before parsing URC
        let (coord_part, qa_part) = if let Some(brace_idx) = rest.find('{') {
            (&rest[..brace_idx], Some(&rest[brace_idx..]))
        } else {
            (rest, None)
        };

        // Extract {via:...} from JSONqa if present
        let (endpoint, _qa_without_via) = if let Some(qa_str) = qa_part {
            let (via, remaining) = extract_via(qa_str);
            let ep = via.map(|v| {
                if v == "repo" {
                    v
                } else {
                    Endpoint::from_str_with_default(&v, DEFAULT_PORT).to_string()
                }
            });
            (ep, if remaining.is_empty() { None } else { Some(remaining) })
        } else {
            (None, None)
        };

        // Validate endpoint requirements per scheme
        if scheme.forbids_endpoint() && endpoint.is_some() {
            return Err(Error::Syntax(Some(
                "Endpoint not allowed for this scheme".to_string(),
            )));
        }

        if scheme.requires_endpoint() && endpoint.is_none() {
            return Err(Error::Syntax(Some(
                "Endpoint required for this scheme".to_string(),
            )));
        }

        // Parse URC (without qa suffix)
        let inner_urc = HpprURC::parse(coord_part.to_string())
            .map_err(|e| Error::Syntax(Some(format!("Invalid URC: {}", e))))?;

        // Parse qa suffix if present (the remaining qa after via extraction)
        // Also include `via` back into the qa for href reconstruction
        let qa = if let Some(qa_str) = qa_part {
            // Parse full qa (including via) for storage
            Some(Qa::parse(qa_str).map_err(|e| Error::Syntax(Some(e.to_string())))?)
        } else {
            None
        };

        Ok((scheme, endpoint, inner_urc, qa))
    }

    /// Extract current (app, location) from inner_urc.
    fn current_app_loc(&self) -> (String, String) {
        match self.inner_urc.group_app_loc() {
            Some((_, Some((app, loc)))) => (app, loc.unwrap_or_default()),
            Some((_, None)) => (String::new(), String::new()),
            None => (String::new(), String::new()),
        }
    }

    /// Extract current (group, location) from inner_urc.
    fn current_group_loc(&self) -> (String, String) {
        match self.inner_urc.group_app_loc() {
            Some((group, Some((_, loc)))) => (group, loc.unwrap_or_default()),
            Some((group, None)) => (group, String::new()),
            None => (String::new(), String::new()),
        }
    }

    /// Extract current (group, app) from inner_urc.
    fn current_group_app(&self) -> (String, String) {
        match self.inner_urc.group_app_loc() {
            Some((group, Some((app, _)))) => (group, app),
            Some((group, None)) => (group, String::new()),
            None => (String::new(), String::new()),
        }
    }

    /// Navigate to a new HPPR URL, mirroring Location::navigate_a_location.
    fn navigate_to(&self, url: BrowserUrl, can_gc: CanGc) {
        let global = self.global();
        let window = global.as_window();
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

    /// Build an HPPR URL string from components.
    ///
    /// Format: `scheme://coord{qa_suffix}` — no endpoint prefix.
    fn build_href(scheme: &str, endpoint: &Option<String>, coord: &str, qa_suffix: &str) -> String {
        let mut href = format!("{}:{}", scheme, coord);
        // Add {via:...} back into the qa suffix if endpoint is present
        if let Some(ep) = endpoint {
            if qa_suffix.is_empty() {
                href.push_str(&format!("{{via:{}}}", ep));
            } else if qa_suffix.starts_with('{') && qa_suffix.ends_with('}') {
                // Merge via into existing qa: {via:ep,existing_keys}
                let inner = &qa_suffix[1..qa_suffix.len() - 1];
                href.push_str(&format!("{{via:{},{}}}", ep, inner));
            } else {
                href.push_str(&format!("{{via:{}}}", ep));
                href.push_str(qa_suffix);
            }
        } else {
            href.push_str(qa_suffix);
        }
        href
    }

    /// Get the current qa suffix string from the URC, excluding `via`.
    fn qa_suffix(&self) -> String {
        use crate::dom::bindings::codegen::Bindings::URCBinding::URCMethods;
        let urc = self.Urc();
        let href = urc.Href().0;
        // URC href is "//g/a/loc{qa}" — extract from first {
        if let Some(idx) = href.find('{') {
            let full_qa = &href[idx..];
            // Strip out `via:...` from the qa
            let (_via, remaining) = extract_via(full_qa);
            remaining
        } else {
            String::new()
        }
    }

    /// Parse and navigate to a new HPPR URL string. Validates before navigating.
    fn navigate_to_href(&self, href: &str, can_gc: CanGc) -> ErrorResult {
        // Validate the URL parses as a valid Address
        let _ = Self::parse(href)?;
        let url = BrowserUrl::parse(href)
            .map_err(|e| Error::Syntax(Some(format!("Invalid URL: {}", e))))?;
        self.navigate_to(url, can_gc);
        Ok(())
    }
}

impl AddressMethods<crate::DomTypeHolder> for Address {
    /// Constructor: parse an HPPR Address string.
    fn Constructor(
        window: &Window,
        proto: Option<HandleObject>,
        can_gc: CanGc,
        input: USVString,
    ) -> Fallible<DomRoot<Self>> {
        let (scheme, endpoint, inner_urc, qa) = Self::parse(&input.0)?;
        let address = Self::new_inherited(scheme, endpoint, inner_urc);
        let global = window.upcast::<GlobalScope>();
        let result = reflect_dom_object_with_proto(Box::new(address), global, proto, can_gc);
        result.urc_cache.or_init(|| {
            URC::new_with_qa(global, result.inner_urc.clone(), qa, can_gc)
        });
        Ok(result)
    }

    /// Returns the full Address string including qa suffix (stringifier).
    fn GetHref(&self) -> Fallible<USVString> {
        let mut href = format!("{}:", self.scheme_name);
        use crate::dom::bindings::codegen::Bindings::URCBinding::URCMethods;
        let urc_href = self.Urc().Href().0;
        // URC href already contains {via:...} if it was in the original qa
        href.push_str(&urc_href);
        Ok(USVString(href))
    }

    /// Set href and navigate to the new address.
    fn SetHref(&self, value: USVString) -> ErrorResult {
        self.navigate_to_href(&value.0, CanGc::note())
    }

    /// Returns the scheme.
    fn GetScheme(&self) -> Fallible<DOMString> {
        Ok(DOMString::from(self.scheme_name.as_str()))
    }

    /// Set scheme and navigate.
    fn SetScheme(&self, value: DOMString) -> ErrorResult {
        let coord = self.inner_urc.to_string();
        let qa = self.qa_suffix();
        let href = Self::build_href(&value.to_string(), &self.endpoint, &coord, &qa);
        self.navigate_to_href(&href, CanGc::note())
    }

    /// Returns the endpoint (host:port) or null.
    fn GetEndpoint(&self) -> Fallible<Option<DOMString>> {
        Ok(self.endpoint.as_ref().map(|e| DOMString::from(e.as_str())))
    }

    /// Set endpoint and navigate.
    ///
    /// Setting endpoint updates the {via:...} in the JSONqa.
    fn SetEndpoint(&self, value: Option<DOMString>) -> ErrorResult {
        let new_ep = value.map(|v| v.to_string());
        let coord = self.inner_urc.to_string();
        let qa = self.qa_suffix();
        let href = Self::build_href(&self.scheme_name, &new_ep, &coord, &qa);
        self.navigate_to_href(&href, CanGc::note())
    }

    /// Returns the coordinate string (//group/app/location) or null.
    fn GetCoordinate(&self) -> Option<DOMString> {
        Some(DOMString::from(self.inner_urc.to_string()))
    }

    /// Returns the URC object (cached for [SameObject]).
    fn Urc(&self) -> DomRoot<URC> {
        self.urc_cache.or_init(|| {
            URC::new(&self.global(), self.inner_urc.clone(), CanGc::note())
        })
    }

    /// Returns the group (convenience accessor).
    fn GetGroup(&self) -> Fallible<Option<DOMString>> {
        Ok(self.inner_urc
            .group_app_loc()
            .map(|(g, _)| DOMString::from(g)))
    }

    /// Set group and navigate.
    fn SetGroup(&self, value: Option<DOMString>) -> ErrorResult {
        let group = value.map(|v| v.to_string()).unwrap_or_default();
        let (app, loc) = self.current_app_loc();
        let coord = build_coordinate(&group, &app, &loc);
        let qa = self.qa_suffix();
        let href = Self::build_href(&self.scheme_name, &self.endpoint, &coord, &qa);
        self.navigate_to_href(&href, CanGc::note())
    }

    /// Returns the app (convenience accessor).
    fn GetApp(&self) -> Fallible<Option<DOMString>> {
        Ok(self.inner_urc
            .group_app_loc()
            .and_then(|(_, rest)| rest.map(|(a, _)| DOMString::from(a))))
    }

    /// Set app and navigate.
    fn SetApp(&self, value: Option<DOMString>) -> ErrorResult {
        let app = value.map(|v| v.to_string()).unwrap_or_default();
        let (group, loc) = self.current_group_loc();
        let coord = build_coordinate(&group, &app, &loc);
        let qa = self.qa_suffix();
        let href = Self::build_href(&self.scheme_name, &self.endpoint, &coord, &qa);
        self.navigate_to_href(&href, CanGc::note())
    }

    /// Returns the location (convenience accessor).
    fn GetLocation(&self) -> Fallible<Option<DOMString>> {
        Ok(self.inner_urc
            .group_app_loc()
            .and_then(|(_, rest)| rest.and_then(|(_, loc)| loc.map(DOMString::from))))
    }

    /// Set location and navigate.
    fn SetLocation(&self, value: Option<DOMString>) -> ErrorResult {
        let loc = value.map(|v| v.to_string()).unwrap_or_default();
        let (group, app) = self.current_group_app();
        let coord = build_coordinate(&group, &app, &loc);
        let qa = self.qa_suffix();
        let href = Self::build_href(&self.scheme_name, &self.endpoint, &coord, &qa);
        self.navigate_to_href(&href, CanGc::note())
    }

    /// Returns whether this is a listing (ends with /).
    fn IsListing(&self) -> bool {
        self.inner_urc.is_listing()
    }

    /// Returns whether this has a direct endpoint ({via:...} present).
    fn HasDirectEndpoint(&self) -> bool {
        self.is_hppr_browsable && self.endpoint.is_some()
    }

    /// Returns the JSONqa metadata as a JS object (delegates to URC).
    fn GetQa(&self, cx: SafeJSContext) -> Fallible<Option<NonNull<JSObject>>> {
        use crate::dom::bindings::codegen::Bindings::URCBinding::URCMethods;
        self.Urc().GetQa(cx)
    }

    /// Sets the JSONqa metadata from a JS object (delegates to URC).
    fn SetQa(&self, cx: SafeJSContext, qa: *mut JSObject) -> Fallible<()> {
        use crate::dom::bindings::codegen::Bindings::URCBinding::URCMethods;
        self.Urc().SetQa(cx, qa)
    }

    /// Returns the fragment value from JSONqa (delegates to URC).
    fn GetFragment(&self) -> Option<DOMString> {
        self.Urc().get_fragment()
    }
}
