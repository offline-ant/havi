/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR Repo Info DOM binding.
//!
//! Provides JavaScript API for querying repo information.
//! Only available on havi:// origin through window.ring0.repo.

use std::rc::Rc;

use dom_struct::dom_struct;
use script_bindings::cformat;
use embedder_traits::{EmbedderMsg, HpprControlRequest, HpprControlResponse};

use crate::dom::bindings::codegen::Bindings::HpprRepoInfoBinding::HpprRepoInfoMethods;
use crate::dom::bindings::error::Error;
use crate::dom::bindings::reflector::{DomGlobal, Reflector, reflect_dom_object};
use crate::dom::bindings::root::DomRoot;
use crate::dom::bindings::str::DOMString;
use crate::dom::envelopehpprclient::EnvelopeHpprClient;
use crate::dom::globalscope::GlobalScope;
use crate::dom::promise::Promise;
use crate::routed_promise::{RoutedPromiseListener, callback_promise};
use crate::script_runtime::CanGc;

/// Check if the current page is on an admin origin (havi://).
fn check_admin_origin(global: &GlobalScope, api_name: &str, can_gc: CanGc) -> Result<(), Rc<Promise>> {
    let url = global.get_url();
    if url.scheme() != "havi" {
        let promise = Promise::new(global, can_gc);
        promise.reject_error(
            Error::Security(Some(format!("{} API only available on havi:// pages", api_name))),
            can_gc,
        );
        return Err(promise);
    }
    Ok(())
}

/// DOM interface for HPPR home repo information.
///
/// Provides methods for querying the hpprd home repo daemon port, home repo path, and status.
#[dom_struct]
pub(crate) struct HpprRepoInfo {
    reflector_: Reflector,
}

impl HpprRepoInfo {
    fn new_inherited() -> Self {
        Self {
            reflector_: Reflector::new(),
        }
    }

    /// Create a new HpprRepoInfo instance.
    pub(crate) fn new(global: &GlobalScope, _client: &EnvelopeHpprClient, can_gc: CanGc) -> DomRoot<Self> {
        reflect_dom_object(Box::new(Self::new_inherited()), global, can_gc)
    }

    /// Send a control operation to the embedder.
    fn send_control_operation(
        &self,
        request: HpprControlRequest,
        promise: &Rc<Promise>,
    ) {
        let global = self.global();
        let window = global.as_window();
        let callback = callback_promise(
            promise,
            self,
            global.task_manager().dom_manipulation_task_source(),
        );

        window.send_to_embedder(EmbedderMsg::HpprControlOperation(
            window.webview_id(),
            global.get_url().to_string(),
            request,
            callback,
        ));
    }
}

impl HpprRepoInfoMethods<crate::DomTypeHolder> for HpprRepoInfo {
    /// Get the hpprd home repo daemon port.
    fn Port(&self) -> Rc<Promise> {
        let can_gc = CanGc::note();
        let global = self.global();

        // Check origin restriction
        if let Err(promise) = check_admin_origin(&global, "repo", can_gc) {
            return promise;
        }

        let promise = Promise::new(&global, can_gc);
        self.send_control_operation(HpprControlRequest::RepoPort, &promise);
        promise
    }

    /// Get the hpprd repo path.
    fn RepoPath(&self) -> Rc<Promise> {
        let can_gc = CanGc::note();
        let global = self.global();

        // Check origin restriction
        if let Err(promise) = check_admin_origin(&global, "repo", can_gc) {
            return promise;
        }

        let promise = Promise::new(&global, can_gc);
        self.send_control_operation(HpprControlRequest::RepoPathQuery, &promise);
        promise
    }

    /// Get home repo status: "embedded" or "external".
    fn Status(&self) -> Rc<Promise> {
        let can_gc = CanGc::note();
        let global = self.global();

        // Check origin restriction
        if let Err(promise) = check_admin_origin(&global, "repo", can_gc) {
            return promise;
        }

        let promise = Promise::new(&global, can_gc);
        self.send_control_operation(HpprControlRequest::RepoStatus, &promise);
        promise
    }
}

impl RoutedPromiseListener<HpprControlResponse> for HpprRepoInfo {
    fn handle_response(&self, response: HpprControlResponse, promise: &Rc<Promise>, can_gc: CanGc) {
        match response {
            HpprControlResponse::Port(port) => {
                promise.resolve_native(&port, can_gc);
            },
            HpprControlResponse::RepoPath(path) => {
                promise.resolve_native(&DOMString::from(path), can_gc);
            },
            HpprControlResponse::RepoStatus(status) => {
                promise.resolve_native(&DOMString::from(status), can_gc);
            },
            HpprControlResponse::Error(msg) => {
                promise.reject_error(Error::Type(cformat!("{}", msg)), can_gc);
            },
            HpprControlResponse::Ok |
            HpprControlResponse::AdminCredential { .. } => {
                promise.reject_error(
                    Error::Type(c"Unexpected response type for repo info".to_owned()),
                    can_gc,
                );
            },
        }
    }
}
