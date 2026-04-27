/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR Client - thin wrapper around EnvelopeHpprClient.
//!
//! Returns values directly (no HpprResult wrapper).
//! All state and request building lives in EnvelopeHpprClient.

use std::rc::Rc;

use base::generic_channel::GenericCallback;
use dom_struct::dom_struct;
use embedder_traits::{EmbedderMsg, HpprControlRequest, HpprControlResponse};
use hppr_client::Signer;
use js::jsval::UndefinedValue;
use net_traits::HpprProtocolResponse;

use crate::dom::bindings::codegen::Bindings::HpprClientBinding::{HpprAddOptions, HpprClientMethods};
use crate::dom::bindings::codegen::Bindings::StreamPubBinding::StreamPubOptions;
use crate::dom::bindings::error::{Error, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::refcounted::{Trusted, TrustedPromise};
use crate::dom::bindings::reflector::{DomGlobal, Reflector, reflect_dom_object};
use crate::dom::bindings::root::{Dom, DomRoot };

use script_bindings::trace::RootedTraceableBox;
use crate::dom::bindings::str::{DOMString, USVString};
use crate::dom::envelopehpprclient::{EnvelopeHpprClient, allow_privileged_connect, resolve_connect_params};
use crate::dom::globalscope::GlobalScope;
use crate::dom::hpprerror::HpprError;
use crate::dom::hpprpacket::HpprPacket;
use crate::dom::hpprresult::HpprResult;
use crate::dom::promise::Promise;
use crate::dom::streampub::StreamPub;
use crate::dom::streamsub::StreamSub;
use crate::dom::watchsocket::WatchSocket;
use crate::dom::window::Window;
use crate::realms::enter_realm;
use crate::routed_promise::{RoutedPromiseListener, callback_promise};
use crate::script_runtime::CanGc;
use script_bindings::cformat;

/// HPPR client that returns values directly.
///
/// Thin wrapper around EnvelopeHpprClient.
#[dom_struct]
pub(crate) struct HpprClient {
    reflector_: Reflector,
    inner: Dom<EnvelopeHpprClient>,
}

impl HpprClient {
    fn new_inherited(inner: &EnvelopeHpprClient) -> Self {
        Self {
            reflector_: Reflector::new(),
            inner: Dom::from_ref(inner),
        }
    }

    /// Create a new HpprClient wrapping an EnvelopeHpprClient.
    pub(crate) fn new(global: &GlobalScope, inner: &EnvelopeHpprClient, can_gc: CanGc) -> DomRoot<Self> {
        reflect_dom_object(Box::new(Self::new_inherited(inner)), global, can_gc)
    }

    /// Create a new HpprClient with a signer.
    pub(crate) fn new_with_signer(
        global: &GlobalScope,
        signer: Signer,
        endpoint: String,
        invalid_reason: Option<String>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        let inner = EnvelopeHpprClient::new(global, signer, endpoint, invalid_reason, can_gc);
        Self::new(global, &inner, can_gc)
    }

    /// Create a new HpprClient backed by the browser-owned committed
    /// repo-source path.
    pub(crate) fn new_local_committed_source(
        global: &GlobalScope,
        invalid_reason: Option<String>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        let inner = EnvelopeHpprClient::new_local_committed_source(global, invalid_reason, can_gc);
        Self::new(global, &inner, can_gc)
    }

    /// Create a new browser-mediated named-client handle.
    pub(crate) fn new_named_client(
        global: &GlobalScope,
        name: String,
        invalid_reason: Option<String>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        let inner = EnvelopeHpprClient::new_named_client(global, name, invalid_reason, can_gc);
        Self::new(global, &inner, can_gc)
    }
}

impl HpprClientMethods<crate::DomTypeHolder> for HpprClient {
    /// HpprClient.connect(endpoint, identity?) - create client to remote endpoint.
    ///
    /// Identity string follows Signer::parse() format. Omitted or empty = anyone.
    fn Connect(window: &Window, endpoint: DOMString, identity: Option<DOMString>) -> Fallible<Rc<Promise>> {
        let global = window.upcast::<GlobalScope>();
        let can_gc = CanGc::note();
        let promise = Promise::new(global, can_gc);

        if let Some((endpoint_str, signer)) =
            resolve_connect_params(window, &endpoint, identity.as_ref(), &promise, can_gc)
        {
            let inner = EnvelopeHpprClient::new(global, signer, endpoint_str, None, can_gc);
            let client = Self::new(global, &inner, can_gc);
            promise.resolve_native(&*client, can_gc);
        }
        Ok(promise)
    }

    fn Named(window: &Window, name: DOMString) -> Fallible<Rc<Promise>> {
        let global = window.upcast::<GlobalScope>();
        let can_gc = CanGc::note();
        let promise = Promise::new(global, can_gc);
        let client_name = name.to_string();
        if client_name.is_empty() {
            promise.reject_error(
                Error::Type(c"Invalid named client: empty string".to_owned()),
                can_gc,
            );
            return Ok(promise);
        }

        let task_source = global.task_manager().dom_manipulation_task_source().to_sendable();
        let mut trusted_promise = Some(TrustedPromise::new(promise.clone()));
        let trusted_window = Trusted::new(window);
        let request_name = client_name.clone();
        let callback = GenericCallback::new(move |message| {
            let Some(trusted_promise) = trusted_promise.take() else {
                error!("HpprClient.named callback called twice");
                return;
            };
            let trusted_window = trusted_window.clone();
            let client_name = request_name.clone();
            task_source.queue(task!(hppr_named_client: move || {
                let promise = trusted_promise.root();
                let window = trusted_window.root();
                let global = window.upcast::<GlobalScope>();
                match message {
                    Ok(HpprControlResponse::Ok) => {
                        let client = HpprClient::new_named_client(
                            global,
                            client_name,
                            None,
                            CanGc::note(),
                        );
                        promise.resolve_native(&*client, CanGc::note());
                    },
                    Ok(HpprControlResponse::Error(error)) => {
                        promise.reject_error(Error::Type(cformat!("{}", error)), CanGc::note());
                    },
                    Ok(other) => {
                        promise.reject_error(
                            Error::Type(cformat!(
                                "Unexpected named-client response: {:?}",
                                other
                            )),
                            CanGc::note(),
                        );
                    },
                    Err(_) => {
                        promise.reject_error(
                            Error::Type(c"Named-client authorization callback failed".to_owned()),
                            CanGc::note(),
                        );
                    },
                }
            }));
        })
        .expect("Could not create HpprClient.named callback");

        window.send_to_embedder(EmbedderMsg::HpprControlOperation(
            window.webview_id(),
            global.get_url().to_string(),
            HpprControlRequest::NamedClientAuthorize {
                client_name,
            },
            callback,
        ));
        Ok(promise)
    }

    /// HpprClient.connectRing2Password(endpoint, group, username, password)
    /// - create client to remote endpoint with a Ring2 adhoc signer.
    /// - derive the signer locally from group, username, and password.
    fn ConnectRing2Password(
        window: &Window,
        endpoint: DOMString,
        group: DOMString,
        username: DOMString,
        password: DOMString,
    ) -> Fallible<Rc<Promise>> {
        let global = window.upcast::<GlobalScope>();
        let can_gc = CanGc::note();
        let promise = Promise::new(global, can_gc);

        if !allow_privileged_connect(window) {
            promise.reject_error(
                Error::Security(Some(
                    "HpprClient.connectRing2Password() is only available to privileged helper pages"
                        .to_string(),
                )),
                can_gc,
            );
            return Ok(promise);
        }

        let endpoint_str = endpoint.to_string();
        if endpoint_str.is_empty() {
            promise.reject_error(
                Error::Type(c"Invalid endpoint: empty string".to_owned()),
                can_gc,
            );
            return Ok(promise);
        }

        let group_str = group.to_string();
        let username_str = username.to_string();
        let password_str = password.to_string();
        let credential_input = format!("{}/{}#{}", group_str, username_str, password_str);
        let signing_key = match hppr_client::derive_ring2_adhoc_signing_key(&credential_input) {
            Ok(key) => key,
            Err(e) => {
                promise.reject_error(
                    Error::Type(cformat!("Invalid Ring2 adhoc identity: {}", e)),
                    can_gc,
                );
                return Ok(promise);
            }
        };
        let signer = Signer::ring2(&group_str, &signing_key);
        let inner = EnvelopeHpprClient::new(global, signer, endpoint_str, None, can_gc);
        let client = Self::new(global, &inner, can_gc);
        promise.resolve_native(&*client, can_gc);
        Ok(promise)
    }

    /// Convert to EnvelopeHpprClient (returns HpprResult with envelopes).
    fn Envelope(&self) -> DomRoot<EnvelopeHpprClient> {
        let invalid_reason = self.inner.invalid_reason().map(|s| s.to_string());
        match self.inner.clone_backend() {
            super::envelopehpprclient::EnvelopeHpprClientBackend::Remote { signer, endpoint } => {
                EnvelopeHpprClient::new(
                    &self.global(),
                    signer,
                    endpoint,
                    invalid_reason,
                    CanGc::note(),
                )
            },
            super::envelopehpprclient::EnvelopeHpprClientBackend::LocalCommittedSource => {
                EnvelopeHpprClient::new_local_committed_source(
                    &self.global(),
                    invalid_reason,
                    CanGc::note(),
                )
            },
            super::envelopehpprclient::EnvelopeHpprClientBackend::NamedClient { name } => {
                EnvelopeHpprClient::new_named_client(
                    &self.global(),
                    name,
                    invalid_reason,
                    CanGc::note(),
                )
            },
        }
    }

    fn Endpoint(&self) -> DOMString {
        DOMString::from(self.inner.endpoint())
    }

    fn GetAccount(&self) -> Option<DOMString> {
        self.inner
            .signer()
            .and_then(|signer| signer.ring1_name())
            .map(DOMString::from)
    }

    fn GetGroup(&self) -> Option<DOMString> {
        self.inner
            .signer()
            .and_then(|signer| signer.group())
            .map(DOMString::from)
    }

    fn GetRing1Name(&self) -> Option<DOMString> {
        self.inner
            .signer()
            .and_then(|signer| signer.ring1_name())
            .map(DOMString::from)
    }

    hppr_dispatch!(Get, (.inner), do_get, &urc.to_string(); urc: USVString);
    hppr_dispatch!(List, (.inner), do_list, &urc.to_string(); urc: USVString);
    hppr_dispatch!(Headers, (.inner), do_headers, &urc.to_string(); urc: USVString);
    hppr_dispatch!(Tips, (.inner), do_tips, &urc.to_string(); urc: USVString);
    hppr_dispatch!(Members, (.inner), do_members, &urc.to_string(); urc: USVString);
    hppr_dispatch!(Store, (.inner), do_store, packet.as_bytes(); packet: &HpprPacket);
    hppr_dispatch!(Detach, (.inner), do_detach, &hash.to_string(); hash: DOMString);

    fn Add(&self, options: RootedTraceableBox<HpprAddOptions>) -> Rc<Promise> {
        let can_gc = CanGc::note();
        let global = self.global();
        let promise = Promise::new(&global, can_gc);
        if self.inner.reject_if_invalid(&promise, can_gc) { return promise; }

        let callback = callback_promise::<HpprProtocolResponse, _>(
            &promise, self, global.task_manager().dom_manipulation_task_source(),
        );
        if let Err(e) = self.inner.do_add(&options, callback) {
            promise.reject_error(e, can_gc);
        }
        promise
    }

    hppr_dispatch!(Hello, (.inner), do_hello);

    fn Watch(&self, urc: USVString) -> DomRoot<WatchSocket> {
        self.inner.do_watch(&urc.to_string(), CanGc::note())
    }

    fn StreamPub(&self, prefix: USVString, options: &StreamPubOptions) -> DomRoot<StreamPub> {
        self.inner.do_stream_pub(&prefix.to_string(), options, CanGc::note())
    }

    fn StreamSub(&self, prefix: USVString) -> DomRoot<StreamSub> {
        self.inner.do_stream_sub(&prefix.to_string(), CanGc::note())
    }
}

/// HpprClient resolves with the value directly (no envelopes).
impl RoutedPromiseListener<HpprProtocolResponse> for HpprClient {
    fn handle_response(&self, response: HpprProtocolResponse, promise: &Rc<Promise>, can_gc: CanGc) {
        let global = self.global();

        match response {
            Err(protocol_error) => {
                let hppr_error = HpprError::from_protocol_error(&global, &protocol_error, can_gc);
                promise.reject_native(&*hppr_error, can_gc);
            },
            Ok(response) => match HpprResult::value_from_response(&global, response, can_gc) {
                Ok(value) => {
                    let cx = GlobalScope::get_cx();
                    let _ac = enter_realm(&*global);
                    rooted!(in(*cx) let mut rval = UndefinedValue());
                    rval.set(value);
                    promise.resolve(cx, rval.handle(), can_gc);
                },
                Err(e) => promise.reject_error(e, can_gc),
            },
        }
    }
}
