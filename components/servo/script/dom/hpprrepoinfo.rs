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

use crate::script::dom::bindings::codegen::GenericBindings::HpprRepoInfoBinding::HpprRepoInfoMethods;
use crate::script::dom::bindings::error::Error;
use crate::script::dom::bindings::reflector::{DomGlobal, Reflector, reflect_dom_object};
use crate::script::dom::bindings::root::DomRoot;
use crate::script::dom::bindings::str::DOMString;
use crate::script::dom::globalscope::GlobalScope;
use crate::script::dom::promise::Promise;
use crate::script::routed_promise::{RoutedPromiseListener, callback_promise};
use crate::script::script_runtime::CanGc;

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
    pub(crate) fn new(global: &GlobalScope, can_gc: CanGc) -> DomRoot<Self> {
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

impl HpprRepoInfo {
    /// Check admin origin, create promise, and send a control operation.
    fn admin_control(&self, request: HpprControlRequest) -> Rc<Promise> {
        let can_gc = CanGc::note();
        let global = self.global();
        if let Err(promise) = check_admin_origin(&global, "repo", can_gc) {
            return promise;
        }
        let promise = Promise::new(&global, can_gc);
        self.send_control_operation(request, &promise);
        promise
    }
}

impl HpprRepoInfoMethods<crate::DomTypeHolder> for HpprRepoInfo {
    fn Port(&self) -> Rc<Promise> {
        self.admin_control(HpprControlRequest::RepoPort)
    }

    fn RepoPath(&self) -> Rc<Promise> {
        self.admin_control(HpprControlRequest::RepoPathQuery)
    }

    fn Status(&self) -> Rc<Promise> {
        self.admin_control(HpprControlRequest::RepoStatus)
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
            HpprControlResponse::AdminCredential { .. } |
            HpprControlResponse::Resolve(_) |
            HpprControlResponse::EmbedResolve(_) => {
                promise.reject_error(
                    Error::Type(c"Unexpected response type for repo info".to_owned()),
                    can_gc,
                );
            },
        }
    }
}
