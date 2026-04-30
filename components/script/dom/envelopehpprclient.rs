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
use embedder_traits::{EmbedderMsg, HpprControlRequest, HpprControlResponse};
use hppr_client::add_coords_to_pac_headers;
use hppr_client::parse_via;
use hppr_client::Signer;
use net_traits::{CoreResourceMsg, HpprProtocolError, HpprProtocolResponse, HpprRequest};

use script_bindings::trace::RootedTraceableBox;
use crate::dom::bindings::codegen::Bindings::EnvelopeHpprClientBinding::EnvelopeHpprClientMethods;
use crate::dom::bindings::codegen::Bindings::HpprClientBinding::HpprAddOptions;
use crate::dom::bindings::codegen::Bindings::StreamPubBinding::StreamPubOptions;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::codegen::UnionTypes::StringOrStringSequence;
use crate::dom::bindings::error::{Error, Fallible};
use crate::dom::bindings::reflector::{DomGlobal, Reflector, reflect_dom_object};
use crate::dom::bindings::root::DomRoot;
use crate::dom::bindings::str::{DOMString, USVString};
use crate::dom::bindings::trace::NoTrace;
use crate::dom::globalscope::GlobalScope;
use crate::dom::hpprclient::HpprClient;
use crate::dom::hpprerror::HpprError;
use crate::dom::hpprpacket::HpprPacket;
use crate::dom::hpprresult::HpprResult;
use crate::dom::promise::Promise;
use crate::dom::streampub::StreamPub;
use crate::dom::streamsub::StreamSub;
use crate::dom::watchsocket::WatchSocket;
use crate::dom::window::Window;
use crate::routed_promise::{RoutedPromiseListener, callback_promise};
use crate::script_runtime::CanGc;
use script_bindings::cformat;

pub(crate) fn allow_privileged_connect(window: &Window) -> bool {
    matches!(
        window.upcast::<GlobalScope>().get_url().scheme(),
        "havi" | "hppr-browse" | "hppr-sandbox"
    )
}

/// Parse endpoint and identity for Connect(), rejecting the promise on error.
///
/// Returns (endpoint, signer) on success, or rejects the promise and returns None.
pub(crate) fn resolve_connect_params(
    window: &Window,
    endpoint: &DOMString,
    identity: Option<&DOMString>,
    promise: &Rc<Promise>,
    can_gc: CanGc,
) -> Option<(String, Signer)> {
    if !allow_privileged_connect(window) {
        promise.reject_error(
            Error::Security(Some(
                "HpprClient.connect() is only available to privileged helper pages".to_string(),
            )),
            can_gc,
        );
        return None;
    }
    let endpoint_str = endpoint.to_string();
    if endpoint_str.is_empty() {
        promise.reject_error(
            Error::Type(c"Invalid endpoint: empty string".to_owned()),
            can_gc,
        );
        return None;
    }

    let identity_str = identity.map(|s| s.to_string()).unwrap_or_default();
    let signer = match Signer::parse(&identity_str) {
        Ok(s) => s,
        Err(e) => {
            promise.reject_error(
                Error::Type(cformat!("Invalid identity: {}", e)),
                can_gc,
            );
            return None;
        }
    };

    Some((endpoint_str, signer))
}

#[derive(Clone)]
pub(crate) enum EnvelopeHpprClientBackend {
    Remote {
        signer: Signer,
        endpoint: String,
    },
    LocalCommittedSource,
    NamedClient {
        name: String,
    },
}

/// Envelope HPPR client
///
/// Contains all client state and request building logic.
/// EnvelopeHpprClient returns HpprResult; HpprClient wraps this and extracts values directly.
#[dom_struct]
pub(crate) struct EnvelopeHpprClient {
    reflector_: Reflector,
    /// Client backend.
    #[ignore_malloc_size_of = "client backend"]
    backend: NoTrace<EnvelopeHpprClientBackend>,
    /// When set, the client is unusable and should fail fast with this reason
    invalid_reason: NoTrace<Option<String>>,
}

impl EnvelopeHpprClient {
    fn new_inherited(
        backend: EnvelopeHpprClientBackend,
        invalid_reason: Option<String>,
    ) -> Self {
        Self {
            reflector_: Reflector::new(),
            backend: NoTrace(backend),
            invalid_reason: NoTrace(invalid_reason),
        }
    }

    fn new_with_backend(
        global: &GlobalScope,
        backend: EnvelopeHpprClientBackend,
        invalid_reason: Option<String>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(Self::new_inherited(backend, invalid_reason)),
            global,
            can_gc,
        )
    }

    /// Create a new EnvelopeHpprClient backed by a remote endpoint and signer.
    pub fn new(
        global: &GlobalScope,
        signer: Signer,
        endpoint: String,
        invalid_reason: Option<String>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        Self::new_with_backend(
            global,
            EnvelopeHpprClientBackend::Remote { signer, endpoint },
            invalid_reason,
            can_gc,
        )
    }

    /// Create a new EnvelopeHpprClient backed by the browser-owned committed
    /// repo-source path.
    pub(crate) fn new_local_committed_source(
        global: &GlobalScope,
        invalid_reason: Option<String>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        Self::new_with_backend(
            global,
            EnvelopeHpprClientBackend::LocalCommittedSource,
            invalid_reason,
            can_gc,
        )
    }

    /// Create a new EnvelopeHpprClient backed by a browser-mediated named
    /// client handle.
    pub(crate) fn new_named_client(
        global: &GlobalScope,
        name: String,
        invalid_reason: Option<String>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        Self::new_with_backend(
            global,
            EnvelopeHpprClientBackend::NamedClient { name },
            invalid_reason,
            can_gc,
        )
    }

    // ========== Accessors ==========

    pub(crate) fn clone_backend(&self) -> EnvelopeHpprClientBackend {
        self.backend.0.clone()
    }

    pub(crate) fn signer(&self) -> Option<&Signer> {
        match &self.backend.0 {
            EnvelopeHpprClientBackend::Remote { signer, .. } => Some(signer),
            EnvelopeHpprClientBackend::LocalCommittedSource |
            EnvelopeHpprClientBackend::NamedClient { .. } => None,
        }
    }

    pub(crate) fn endpoint(&self) -> Option<&str> {
        match &self.backend.0 {
            EnvelopeHpprClientBackend::Remote { endpoint, .. } => Some(endpoint),
            EnvelopeHpprClientBackend::LocalCommittedSource => None,
            EnvelopeHpprClientBackend::NamedClient { .. } => None,
        }
    }

    pub(crate) fn invalid_reason(&self) -> Option<&str> {
        self.invalid_reason.0.as_deref()
    }

    // ========== Internal helpers ==========

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

    fn protocol_error(detail: impl Into<String>) -> HpprProtocolError {
        HpprProtocolError {
            error_type: "INTERNAL".to_string(),
            detail: detail.into(),
            fatal: false,
        }
    }

    /// Send a protocol request through the backend for this client.
    pub(crate) fn send_protocol_request(
        &self,
        request: HpprRequest,
        callback: GenericCallback<HpprProtocolResponse>,
    ) {
        let global = self.global();
        match &self.backend.0 {
            EnvelopeHpprClientBackend::Remote { endpoint, signer } => {
                let via = match parse_via(endpoint) {
                    Ok(v) => v,
                    Err(error) => {
                        let _ = callback.send(Err(Self::protocol_error(format!(
                            "invalid endpoint '{}': {}",
                            endpoint, error
                        ))));
                        return;
                    },
                };
                let _ = global
                    .core_resource_thread()
                    .send(CoreResourceMsg::HpprOperation {
                        endpoint: via,
                        signer: signer.clone(),
                        request,
                        callback,
                    });
            },
            EnvelopeHpprClientBackend::LocalCommittedSource => {
                let bridge = GenericCallback::new(move |message| {
                    let response = match message.unwrap() {
                        HpprControlResponse::CommittedSourceOperation(response) => response,
                        HpprControlResponse::Error(error) => Err(Self::protocol_error(error)),
                        other => Err(Self::protocol_error(format!(
                            "unexpected committed source response: {:?}",
                            other
                        ))),
                    };
                    let _ = callback.send(response);
                })
                .expect("Could not create committed-source callback in script.");
                let window = global.as_window();
                window.send_to_embedder(EmbedderMsg::HpprControlOperation(
                    window.webview_id(),
                    global.get_url().to_string(),
                    HpprControlRequest::CommittedSourceOperation { request },
                    bridge,
                ));
            },
            EnvelopeHpprClientBackend::NamedClient { name } => {
                let client_name = name.clone();
                let bridge = GenericCallback::new(move |message| {
                    let response = match message.unwrap() {
                        HpprControlResponse::NamedClientOperation(response) => response,
                        HpprControlResponse::Error(error) => Err(Self::protocol_error(error)),
                        other => Err(Self::protocol_error(format!(
                            "unexpected named client response: {:?}",
                            other
                        ))),
                    };
                    let _ = callback.send(response);
                })
                .expect("Could not create named-client callback in script.");
                let window = global.as_window();
                window.send_to_embedder(EmbedderMsg::HpprControlOperation(
                    window.webview_id(),
                    global.get_url().to_string(),
                    HpprControlRequest::NamedClientOperation {
                        client_name,
                        request,
                    },
                    bridge,
                ));
            },
        }
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
        match &self.backend.0 {
            EnvelopeHpprClientBackend::Remote { endpoint, signer } => WatchSocket::new(
                &global,
                endpoint,
                signer.clone(),
                urc.to_string(),
                can_gc,
            ),
            EnvelopeHpprClientBackend::LocalCommittedSource => {
                let ws = WatchSocket::new_pending(&global, urc.to_string(), can_gc);
                ws.fail_with_error(
                    "browser-local committed source watch is not implemented yet",
                    can_gc,
                );
                ws
            },
            EnvelopeHpprClientBackend::NamedClient { .. } => {
                let ws = WatchSocket::new_pending(&global, urc.to_string(), can_gc);
                ws.fail_with_error(
                    "browser-mediated named client watch is not implemented yet",
                    can_gc,
                );
                ws
            },
        }
    }

    /// Create a StreamPub for the given prefix.
    pub(crate) fn do_stream_pub(&self, prefix: &str, options: &StreamPubOptions, can_gc: CanGc) -> DomRoot<StreamPub> {
        let global = self.global();
        if let Some(reason) = self.invalid_reason.0.as_deref() {
            let si = StreamPub::new_pending(&global, prefix.to_string(), can_gc);
            si.fail_with_error(reason, can_gc);
            return si;
        }
        let publisher_params = match StreamPub::publisher_params_from_options(options) {
            Ok(params) => params,
            Err(reason) => {
                let si = StreamPub::new_pending(&global, prefix.to_string(), can_gc);
                si.fail_with_error(reason, can_gc);
                return si;
            }
        };
        match &self.backend.0 {
            EnvelopeHpprClientBackend::Remote { endpoint, signer } => StreamPub::new(
                &global,
                endpoint,
                signer.clone(),
                prefix.to_string(),
                publisher_params,
                can_gc,
            ),
            EnvelopeHpprClientBackend::LocalCommittedSource => {
                let si = StreamPub::new_pending(&global, prefix.to_string(), can_gc);
                si.fail_with_error(
                    "browser-local committed source streaming is not implemented yet",
                    can_gc,
                );
                si
            },
            EnvelopeHpprClientBackend::NamedClient { .. } => {
                let si = StreamPub::new_pending(&global, prefix.to_string(), can_gc);
                si.fail_with_error(
                    "browser-mediated named client streaming is not implemented yet",
                    can_gc,
                );
                si
            },
        }
    }

    /// Create a StreamSub for the given prefix.
    pub(crate) fn do_stream_sub(&self, prefix: &str, can_gc: CanGc) -> DomRoot<StreamSub> {
        let global = self.global();
        if let Some(reason) = self.invalid_reason.0.as_deref() {
            let so = StreamSub::new_pending(&global, prefix.to_string(), can_gc);
            so.fail_with_error(reason, can_gc);
            return so;
        }
        match &self.backend.0 {
            EnvelopeHpprClientBackend::Remote { endpoint, signer } => StreamSub::new(
                &global,
                endpoint,
                signer.clone(),
                prefix.to_string(),
                can_gc,
            ),
            EnvelopeHpprClientBackend::LocalCommittedSource => {
                let so = StreamSub::new_pending(&global, prefix.to_string(), can_gc);
                so.fail_with_error(
                    "browser-local committed source streaming is not implemented yet",
                    can_gc,
                );
                so
            },
            EnvelopeHpprClientBackend::NamedClient { .. } => {
                let so = StreamSub::new_pending(&global, prefix.to_string(), can_gc);
                so.fail_with_error(
                    "browser-mediated named client streaming is not implemented yet",
                    can_gc,
                );
                so
            },
        }
    }
}

// ========== WebIDL Methods ==========

impl EnvelopeHpprClientMethods<crate::DomTypeHolder> for EnvelopeHpprClient {
    /// EnvelopeHpprClient.connect(endpoint, identity?) - create client to remote endpoint.
    ///
    /// Identity string follows Signer::parse() format. Omitted or empty = anyone.
    fn Connect(window: &Window, endpoint: DOMString, identity: Option<DOMString>) -> Fallible<Rc<Promise>> {
        let global = window.upcast::<GlobalScope>();
        let can_gc = CanGc::note();
        let promise = Promise::new(global, can_gc);

        if let Some((endpoint_str, signer)) =
            resolve_connect_params(window, &endpoint, identity.as_ref(), &promise, can_gc)
        {
            let client = Self::new(global, signer, endpoint_str, None, can_gc);
            promise.resolve_native(&*client, can_gc);
        }
        Ok(promise)
    }

    /// Convert to HpprClient (returns values directly instead of HpprResult).
    fn Unpack(&self) -> DomRoot<HpprClient> {
        HpprClient::new(&self.global(), self, CanGc::note())
    }

    fn GetEndpoint(&self) -> Option<DOMString> {
        self.endpoint().map(DOMString::from)
    }

    fn GetAccount(&self) -> Option<DOMString> {
        self.signer().and_then(|signer| signer.ring1_name()).map(DOMString::from)
    }

    fn GetGroup(&self) -> Option<DOMString> {
        self.signer().and_then(|signer| signer.group()).map(DOMString::from)
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

    fn Watch(&self, urc: USVString) -> DomRoot<WatchSocket> {
        self.do_watch(&urc.to_string(), CanGc::note())
    }

    fn StreamPub(&self, prefix: USVString, options: &StreamPubOptions) -> DomRoot<StreamPub> {
        self.do_stream_pub(&prefix.to_string(), options, CanGc::note())
    }

    fn StreamSub(&self, prefix: USVString) -> DomRoot<StreamSub> {
        self.do_stream_sub(&prefix.to_string(), CanGc::note())
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
