/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR Client - thin wrapper around EnvelopeHpprClient.
//!
//! Returns values directly (no HpprResult wrapper).
//! All state and request building lives in EnvelopeHpprClient.

use std::rc::Rc;

use dom_struct::dom_struct;
use hppr_client::Signer;
use js::jsval::UndefinedValue;
use net_traits::HpprProtocolResponse;

use crate::dom::bindings::codegen::Bindings::HpprClientBinding::{HpprAddOptions, HpprClientMethods, HpprRepoOptions};
use crate::dom::bindings::codegen::Bindings::StreamInBinding::StreamInOptions;
use crate::dom::bindings::codegen::Bindings::WindowBinding::WindowMethods;
use crate::dom::bindings::error::{Error, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::{DomGlobal, Reflector, reflect_dom_object};
use crate::dom::bindings::root::{Dom, DomRoot };

use script_bindings::trace::RootedTraceableBox;
use crate::dom::bindings::str::{DOMString, USVString};
use crate::dom::envelopehpprclient::{EnvelopeHpprClient, default_endpoint};
use crate::dom::globalscope::GlobalScope;
use crate::dom::hpprerror::HpprError;
use crate::dom::hpprpacket::HpprPacket;
use crate::dom::hpprresult::HpprResult;
use crate::dom::hpprrepoinfo::HpprRepoInfo;
use crate::dom::promise::Promise;
use crate::dom::streamin::StreamIn;
use crate::dom::streamout::StreamOut;
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

}

impl HpprClientMethods<crate::DomTypeHolder> for HpprClient {
    /// HpprClient.repo(options) - create client with site or elevated credentials.
    ///
    /// Without role: uses site sandbox `HAVI-site:<group>#<app>`
    /// With role: uses elevated `HAVI-role:<app>#<role>`, signed by site key
    fn Home(window: &Window, options: &HpprRepoOptions) -> Fallible<Rc<Promise>> {
        let global = window.upcast::<GlobalScope>();
        let can_gc = CanGc::note();
        let promise = Promise::new(global, can_gc);

        // Get site credentials from document (pre-fetched during page load)
        let url = global.get_url();
        if !matches!(url.scheme(), "hppr" | "hppr-editor") {
            promise.reject_error(
                Error::Type(c"HpprClient.home() requires hppr:// origin".to_owned()),
                can_gc,
            );
            return Ok(promise);
        }

        let endpoint = default_endpoint();

        match window.Document().site_credentials() {
            Some((site_ring1_name, signing_key)) => {
                // Determine the ring1_name to use
                let ring1_name = match &options.role {
                    Some(role_name) => {
                        // Extract app from site_ring1_name (HAVI-site:<group>#<app>)
                        let app = site_ring1_name
                            .strip_prefix("HAVI-site:")
                            .and_then(|s| s.split('#').nth(1))
                            .unwrap_or("");
                        // Use role's ring1_name but site's signing key
                        format!("HAVI-role:{}#{}", app, role_name)
                    }
                    None => site_ring1_name,
                };

                let signer = Signer::ring1(&ring1_name, &signing_key);
                let inner = EnvelopeHpprClient::new(global, signer, endpoint, None, can_gc);
                let client = Self::new(global, &inner, can_gc);
                promise.resolve_native(&*client, can_gc);
            }
            None => {
                promise.reject_error(
                    Error::Type(c"HpprClient.home() failed: site credentials not available".to_owned()),
                    can_gc,
                );
            }
        }
        Ok(promise)
    }

    /// HpprClient.connect(endpoint, identity?) - create client to remote endpoint.
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

        let inner = EnvelopeHpprClient::new(global, signer, endpoint_str, None, can_gc);
        let client = Self::new(global, &inner, can_gc);
        promise.resolve_native(&*client, can_gc);
        Ok(promise)
    }

    /// Convert to EnvelopeHpprClient (returns HpprResult with envelopes).
    fn Envelope(&self) -> DomRoot<EnvelopeHpprClient> {
        // Create a new EnvelopeHpprClient with the same state
        EnvelopeHpprClient::new(
            &self.global(),
            self.inner.signer().clone(),
            self.inner.endpoint().to_string(),
            self.inner.invalid_reason().map(|s| s.to_string()),
            CanGc::note(),
        )
    }

    fn Endpoint(&self) -> DOMString {
        DOMString::from(self.inner.endpoint())
    }

    fn GetAccount(&self) -> Option<DOMString> {
        self.inner.signer().ring1_name().map(|s| DOMString::from(s))
    }

    fn GetGroup(&self) -> Option<DOMString> {
        self.inner.signer().group().map(|s| DOMString::from(s))
    }

    fn GetRing1Name(&self) -> Option<DOMString> {
        self.inner.signer().ring1_name()
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

    fn GetRepo(&self) -> Option<DomRoot<HpprRepoInfo>> {
        self.inner.do_get_repo()
    }

    fn Watch(&self, urc: USVString) -> DomRoot<WatchSocket> {
        self.inner.do_watch(&urc.to_string(), CanGc::note())
    }

    fn StreamIn(&self, prefix: USVString, options: &StreamInOptions) -> DomRoot<StreamIn> {
        self.inner.do_stream_in(&prefix.to_string(), options, CanGc::note())
    }

    fn StreamOut(&self, prefix: USVString) -> DomRoot<StreamOut> {
        self.inner.do_stream_out(&prefix.to_string(), CanGc::note())
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
