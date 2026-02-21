/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Envelope HPPR Client - the real client implementation.
//!
//! Contains all client state, authentication, and request building.
//! HpprClient is a thin wrapper that delegates to this.

use std::rc::Rc;

use base::generic_channel::GenericCallback;
use dom_struct::dom_struct;
use hppr_client::add_coords_to_pac_headers;
use hppr_client::env_target::parse_via;
use hppr_client::Signer;
use net_traits::{HpprRequest, CoreResourceMsg, HpprProtocolResponse};

use script_bindings::trace::RootedTraceableBox;
use crate::dom::bindings::codegen::Bindings::EnvelopeHpprClientBinding::EnvelopeHpprClientMethods;
use crate::dom::bindings::codegen::Bindings::HpprClientBinding::HpprAddOptions;
use crate::dom::bindings::codegen::Bindings::StreamInBinding::StreamInOptions;
use crate::dom::bindings::codegen::Bindings::WindowBinding::WindowMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::codegen::UnionTypes::StringOrStringSequence;
use crate::dom::bindings::error::{Error, Fallible};
use crate::dom::bindings::reflector::{DomGlobal, Reflector, reflect_dom_object};
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::bindings::str::{DOMString, USVString};
use crate::dom::bindings::trace::NoTrace;
use crate::dom::globalscope::GlobalScope;
use crate::dom::hpprclient::HpprClient;
use crate::dom::hpprerror::HpprError;
use crate::dom::hpprpacket::HpprPacket;
use crate::dom::hpprresult::HpprResult;
use crate::dom::hpprrepoinfo::HpprRepoInfo;
use crate::dom::promise::Promise;
use crate::dom::streamin::StreamIn;
use crate::dom::streamout::StreamOut;
use crate::dom::watchsocket::WatchSocket;
use crate::dom::window::Window;
use crate::routed_promise::{RoutedPromiseListener, callback_promise};
use crate::script_runtime::CanGc;
use script_bindings::cformat;

/// Default HPPR socket endpoint.
pub(crate) fn default_endpoint() -> String {
    hppr_client::repo_endpoint().to_string()
}

/// Envelope HPPR client
///
/// Contains all client state and request building logic.
/// EnvelopeHpprClient returns HpprResult; HpprClient wraps this and extracts values directly.
#[dom_struct]
pub(crate) struct EnvelopeHpprClient {
    reflector_: Reflector,
    /// Signer for requests
    #[ignore_malloc_size_of = "hppr_client::Signer doesn't implement MallocSizeOf"]
    signer: NoTrace<Signer>,
    /// Repo endpoint (e.g., "127.0.0.1:4777")
    endpoint: NoTrace<String>,
    /// When set, the client is unusable and should fail fast with this reason
    invalid_reason: NoTrace<Option<String>>,
    /// Lazy-initialized HpprRepoInfo sub-object (only for ring0)
    repo: MutNullableDom<HpprRepoInfo>,
}

impl EnvelopeHpprClient {
    fn new_inherited(
        signer: Signer,
        endpoint: String,
        invalid_reason: Option<String>,
    ) -> Self {
        Self {
            reflector_: Reflector::new(),
            signer: NoTrace(signer),
            endpoint: NoTrace(endpoint),
            invalid_reason: NoTrace(invalid_reason),
            repo: MutNullableDom::new(None),
        }
    }

    /// Create a new EnvelopeHpprClient.
    pub fn new(
        global: &GlobalScope,
        signer: Signer,
        endpoint: String,
        invalid_reason: Option<String>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self::new_inherited(signer, endpoint, invalid_reason)),
            global,
            can_gc,
        )
    }

    // ========== Accessors ==========

    pub(crate) fn signer(&self) -> &Signer {
        &self.signer.0
    }

    pub(crate) fn endpoint(&self) -> &str {
        &self.endpoint.0
    }

    pub(crate) fn invalid_reason(&self) -> Option<&str> {
        self.invalid_reason.0.as_deref()
    }

    // ========== Internal helpers ==========

    /// Clone the signer for sending across threads.
    fn signer_clone(&self) -> Signer {
        self.signer.0.clone()
    }

    /// Reject operations when credentials are missing.
    pub(crate) fn reject_if_invalid(&self, promise: &Rc<Promise>, can_gc: CanGc) -> bool {
        if let Some(reason) = self.invalid_reason.0.as_deref() {
            promise.reject_error(Error::Type(cformat!("{}", reason)), can_gc);
            return true;
        }
        false
    }

    /// Extract page context (group/app/location) from current URL.
    pub(crate) fn get_page_context(&self) -> (String, String, String) {
        let url = self.global().get_url();
        let url_str = url.as_str();
        // Find the "//" coordinate marker after the scheme
        let coord_start = url_str.find("//").unwrap_or(url_str.len());
        let coord = &url_str[coord_start..];
        match hppr_packet::urc::URC::parse(coord.to_string()) {
            Ok(urc) => match urc.group_app_loc() {
                Some((group, rest)) => {
                    let (app, loc) = match rest {
                        Some((a, l)) => (a.to_string(), l.unwrap_or_default().to_string()),
                        None => (String::new(), String::new()),
                    };
                    (group.to_string(), app, loc)
                }
                None => (String::new(), String::new(), String::new()),
            },
            Err(_) => (String::new(), String::new(), String::new()),
        }
    }

    /// Send a protocol request to the resource thread.
    pub(crate) fn send_protocol_request(
        &self,
        request: HpprRequest,
        callback: GenericCallback<HpprProtocolResponse>,
    ) {
        let global = self.global();
        let via = match parse_via(&self.endpoint.0) {
            Ok(v) => v,
            Err(_) => return,
        };
        let _ = global
            .core_resource_thread()
            .send(CoreResourceMsg::HpprOperation {
                endpoint: via,
                signer: self.signer_clone(),
                request,
                callback,
            });
    }

    // ========== Internal do_* methods ==========

    /// Send a GET request.
    pub(crate) fn do_get(
        &self,
        urc: &str,
        callback: GenericCallback<HpprProtocolResponse>,
    ) {
        let request = HpprRequest::Get {
            urc: urc.to_string(),
        };
        self.send_protocol_request(request, callback);
    }

    /// Send a LIST request.
    pub(crate) fn do_list(
        &self,
        urc: &str,
        callback: GenericCallback<HpprProtocolResponse>,
    ) {
        let request = HpprRequest::List {
            urc: urc.to_string(),
        };
        self.send_protocol_request(request, callback);
    }

    /// Send a HEADERS request.
    pub(crate) fn do_headers(
        &self,
        urc: &str,
        callback: GenericCallback<HpprProtocolResponse>,
    ) {
        let request = HpprRequest::Headers {
            urc: urc.to_string(),
        };
        self.send_protocol_request(request, callback);
    }

    /// Send a TIPS request.
    pub(crate) fn do_tips(
        &self,
        urc: &str,
        callback: GenericCallback<HpprProtocolResponse>,
    ) {
        let request = HpprRequest::Tips {
            urc: urc.to_string(),
        };
        self.send_protocol_request(request, callback);
    }

    /// Send a MEMBERS request.
    pub(crate) fn do_members(
        &self,
        args: &str,
        callback: GenericCallback<HpprProtocolResponse>,
    ) {
        let request = HpprRequest::Members {
            args: args.to_string(),
        };
        self.send_protocol_request(request, callback);
    }

    /// Send a STORE request.
    pub(crate) fn do_store(
        &self,
        packet: &[u8],
        callback: GenericCallback<HpprProtocolResponse>,
    ) {
        let request = HpprRequest::Store {
            packet: packet.to_vec(),
        };
        self.send_protocol_request(request, callback);
    }

    /// Send a DETACH request.
    pub(crate) fn do_detach(&self, hash: &str, callback: GenericCallback<HpprProtocolResponse>) {
        let request = HpprRequest::Detach {
            hash: hash.to_string(),
        };
        self.send_protocol_request(request, callback);
    }

    /// Build and send an ADD request. Returns Err if validation fails.
    pub(crate) fn do_add(
        &self,
        options: &HpprAddOptions,

        callback: GenericCallback<HpprProtocolResponse>,
    ) -> Result<(), Error> {
        // Normalize headers from string or array
        let custom_headers: Vec<String> = match &options.headers {
            Some(StringOrStringSequence::String(s)) => s
                .str()
                .lines()
                .filter(|l| !l.is_empty())
                .map(|s| s.to_string())
                .collect(),
            Some(StringOrStringSequence::StringSequence(arr)) => {
                arr.iter().map(|s| s.str().to_string()).collect()
            },
            None => vec![],
        };

        // Detect existing coordinate headers (case-sensitive per spec)
        let (has_group, has_app, has_loc) =
            custom_headers
                .iter()
                .fold((false, false, false), |(g, a, l), h| {
                    (
                        g || h.starts_with("Group:"),
                        a || h.starts_with("App:"),
                        l || h.starts_with("Location:"),
                    )
                });

        // Extract data bytes from the union type
        let data = options.data.as_ref().map(|d| {
            use crate::dom::bindings::codegen::UnionTypes::BlobOrArrayBufferOrArrayBufferViewOrUSVString;
            match d {
                BlobOrArrayBufferOrArrayBufferViewOrUSVString::Blob(blob) => {
                    blob.get_bytes().unwrap_or_default()
                },
                BlobOrArrayBufferOrArrayBufferViewOrUSVString::ArrayBuffer(ab) => ab.to_vec(),
                BlobOrArrayBufferOrArrayBufferViewOrUSVString::ArrayBufferView(abv) => abv.to_vec(),
                BlobOrArrayBufferOrArrayBufferViewOrUSVString::USVString(s) => s.0.as_bytes().to_vec(),
            }
        }).unwrap_or_default();

        // Build headers (fill in coordinates from page context if missing)
        let headers_str = custom_headers.join("\n");
        let headers = if has_group && has_app && has_loc {
            headers_str.into_bytes()
        } else {
            let (page_group, page_app, page_location) = self.get_page_context();
            if page_group.is_empty() || page_app.is_empty() || page_location.is_empty() {
                return Err(Error::Type(
                    c"Missing Group/App/Location headers on non-HPPR page".to_owned(),
                ));
            }
            let current_coord = format!("//{}/{}/{}", page_group, page_app, page_location);
            add_coords_to_pac_headers(&headers_str, &current_coord)
                .map_err(|e| Error::Type(cformat!("Invalid coordinates: {}", e)))?
                .into_bytes()
        };

        let data_opt = if data.is_empty() { None } else { Some(data) };

        let request = HpprRequest::Add {
            headers,
            data: data_opt,
        };
        self.send_protocol_request(request, callback);
        Ok(())
    }

    /// Send a HELLO request.
    pub(crate) fn do_hello(&self, callback: GenericCallback<HpprProtocolResponse>) {
        self.send_protocol_request(HpprRequest::Hello, callback);
    }

    /// Create a WatchSocket for the given URC.
    pub(crate) fn do_watch(&self, urc: &str, can_gc: CanGc) -> DomRoot<WatchSocket> {
        let global = self.global();
        if let Some(reason) = self.invalid_reason.0.as_deref() {
            let ws = WatchSocket::new_pending(&global, urc.to_string(), can_gc);
            ws.fail_with_error(reason, can_gc);
            return ws;
        }
        WatchSocket::new(
            &global,
            &self.endpoint.0,
            self.signer_clone(),
            urc.to_string(),
            can_gc,
        )
    }

    /// Create a StreamIn for the given prefix.
    pub(crate) fn do_stream_in(&self, prefix: &str, options: &StreamInOptions, can_gc: CanGc) -> DomRoot<StreamIn> {
        let global = self.global();
        if let Some(reason) = self.invalid_reason.0.as_deref() {
            let si = StreamIn::new_pending(&global, prefix.to_string(), can_gc);
            si.fail_with_error(reason, can_gc);
            return si;
        }
        let publisher_params = StreamIn::publisher_params_from_options(options);
        StreamIn::new(
            &global,
            &self.endpoint.0,
            self.signer_clone(),
            prefix.to_string(),
            publisher_params,
            can_gc,
        )
    }

    /// Create a StreamOut for the given prefix.
    pub(crate) fn do_stream_out(&self, prefix: &str, can_gc: CanGc) -> DomRoot<StreamOut> {
        let global = self.global();
        if let Some(reason) = self.invalid_reason.0.as_deref() {
            let so = StreamOut::new_pending(&global, prefix.to_string(), can_gc);
            so.fail_with_error(reason, can_gc);
            return so;
        }
        StreamOut::new(
            &global,
            &self.endpoint.0,
            self.signer_clone(),
            prefix.to_string(),
            can_gc,
        )
    }

    /// Get or create the HpprRepoInfo sub-object (admin only).
    pub(crate) fn do_get_repo(&self) -> Option<DomRoot<HpprRepoInfo>> {
        if self.repo.get().is_none() {
            let can_gc = CanGc::note();
            let repo = HpprRepoInfo::new(&self.global(), self, can_gc);
            self.repo.set(Some(&repo));
        }
        self.repo.get()
    }
}

// ========== WebIDL Methods ==========

impl EnvelopeHpprClientMethods<crate::DomTypeHolder> for EnvelopeHpprClient {
    /// EnvelopeHpprClient.home() - create client with site sandbox credentials.
    ///
    /// Uses the current page's site Ring1 (HAVI-site:<group>#<app>) with
    /// seal-based authentication via the site's signing key.
    fn Home(window: &Window) -> Fallible<Rc<Promise>> {
        let global = window.upcast::<GlobalScope>();
        let can_gc = CanGc::note();
        let promise = Promise::new(global, can_gc);

        // Get site credentials from document (pre-fetched during page load)
        let url = global.get_url();
        if !matches!(url.scheme(), "hppr" | "hppr-editor") {
            promise.reject_error(
                Error::Type(c"EnvelopeHpprClient.home() requires hppr:// origin".to_owned()),
                can_gc,
            );
            return Ok(promise);
        }

        let endpoint = default_endpoint();

        match window.Document().site_credentials() {
            Some((ring1_name, signing_key)) => {
                let signer = Signer::ring1(&ring1_name, &signing_key);
                let client = Self::new(global, signer, endpoint, None, can_gc);
                promise.resolve_native(&*client, can_gc);
            }
            None => {
                promise.reject_error(
                    Error::Type(c"EnvelopeHpprClient.home() failed: site credentials not available".to_owned()),
                    can_gc,
                );
            }
        }
        Ok(promise)
    }

    /// EnvelopeHpprClient.connect(endpoint, identity?) - create client to remote endpoint.
    ///
    /// Identity string follows Signer::parse() format. Omitted or empty = anyone.
    fn Connect(window: &Window, endpoint: DOMString, identity: Option<DOMString>) -> Fallible<Rc<Promise>> {
        let global = window.upcast::<GlobalScope>();
        let can_gc = CanGc::note();
        let promise = Promise::new(global, can_gc);

        let endpoint_str = endpoint.to_string();

        // Validate endpoint format (host:port or just host)
        if endpoint_str.is_empty() {
            promise.reject_error(
                Error::Type(c"Invalid endpoint: empty string".to_owned()),
                can_gc,
            );
            return Ok(promise);
        }

        // Parse identity string (empty/missing = anyone)
        let identity_str = identity.as_ref().map(|s| s.to_string()).unwrap_or_default();
        let signer = match Signer::parse(&identity_str) {
            Ok(s) => s,
            Err(e) => {
                promise.reject_error(
                    Error::Type(cformat!("Invalid identity: {}", e)),
                    can_gc,
                );
                return Ok(promise);
            }
        };

        let client = Self::new(global, signer, endpoint_str, None, can_gc);
        promise.resolve_native(&*client, can_gc);
        Ok(promise)
    }

    /// Convert to HpprClient (returns values directly instead of HpprResult).
    fn Unpack(&self) -> DomRoot<HpprClient> {
        HpprClient::new(&self.global(), self, CanGc::note())
    }

    fn Endpoint(&self) -> DOMString {
        DOMString::from(&*self.endpoint.0)
    }

    fn GetAccount(&self) -> Option<DOMString> {
        self.signer.0.ring1_name().map(|v| DOMString::from(v))
    }

    fn GetGroup(&self) -> Option<DOMString> {
        self.signer.0.group().map(|v| DOMString::from(v))
    }

    hppr_dispatch!(Get, (), do_get, &urc.to_string(); urc: USVString);
    hppr_dispatch!(List, (), do_list, &urc.to_string(); urc: USVString);
    hppr_dispatch!(Headers, (), do_headers, &urc.to_string(); urc: USVString);
    hppr_dispatch!(Tips, (), do_tips, &urc.to_string(); urc: USVString);
    hppr_dispatch!(Members, (), do_members, &urc.to_string(); urc: USVString);
    hppr_dispatch!(Store, (), do_store, packet.as_bytes(); packet: &HpprPacket);
    hppr_dispatch!(Detach, (), do_detach, &hash.to_string(); hash: DOMString);

    fn Add(&self, options: RootedTraceableBox<HpprAddOptions>) -> Rc<Promise> {
        let can_gc = CanGc::note();
        let global = self.global();
        let promise = Promise::new(&global, can_gc);
        if self.reject_if_invalid(&promise, can_gc) {
            return promise;
        }

        let callback = callback_promise::<HpprProtocolResponse, _>(
            &promise,
            self,
            global.task_manager().dom_manipulation_task_source(),
        );
        if let Err(e) = self.do_add(&options, callback) {
            promise.reject_error(e, can_gc);
        }
        promise
    }

    hppr_dispatch!(Hello, (), do_hello);

    fn GetRepo(&self) -> Option<DomRoot<HpprRepoInfo>> {
        self.do_get_repo()
    }

    fn Watch(&self, urc: USVString) -> DomRoot<WatchSocket> {
        self.do_watch(&urc.to_string(), CanGc::note())
    }

    fn StreamIn(&self, prefix: USVString, options: &StreamInOptions) -> DomRoot<StreamIn> {
        self.do_stream_in(&prefix.to_string(), options, CanGc::note())
    }

    fn StreamOut(&self, prefix: USVString) -> DomRoot<StreamOut> {
        self.do_stream_out(&prefix.to_string(), CanGc::note())
    }
}

/// EnvelopeHpprClient resolves with HpprResult (includes envelopes).
impl RoutedPromiseListener<HpprProtocolResponse> for EnvelopeHpprClient {
    fn handle_response(&self, response: HpprProtocolResponse, promise: &Rc<Promise>, can_gc: CanGc) {
        match response {
            Err(protocol_error) => {
                let global = self.global();
                let hppr_error = HpprError::from_protocol_error(&global, &protocol_error, can_gc);
                promise.reject_native(&*hppr_error, can_gc);
            },
            Ok(response) => {
                let global = self.global();
                match HpprResult::new(&global, response, can_gc) {
                    Ok(result) => promise.resolve_native(&*result, can_gc),
                    Err(e) => promise.reject_error(e, can_gc),
                }
            }
        }
    }
}
