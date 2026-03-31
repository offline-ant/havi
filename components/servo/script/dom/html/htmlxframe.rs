/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;
use std::rc::Rc;

use base::generic_channel::GenericCallback;
use base::id::{BrowsingContextId, PipelineId, WebViewId};
use crate::constellation::{
    IFrameLoadInfo, IFrameLoadInfoWithData, JsEvalResult, LoadData, LoadOrigin,
    NavigationHistoryBehavior, ScriptToConstellationMessage,
};
use content_security_policy::sandboxing_directive::SandboxingFlagSet;
use dom_struct::dom_struct;
use embedder_traits::{
    EmbedderMsg, HpprControlRequest, HpprControlResponse, HpprEmbedResolveResponse,
    ViewportDetails,
};
use html5ever::{LocalName, Prefix, local_name, ns};
use js::context::JSContext;
use js::rust::HandleObject;
use net_traits::request::Destination;
use profile_traits::ipc as ProfiledIpc;
use crate::script::{NewPipelineInfo, UpdatePipelineIdReason};
use servo_url::BrowserUrl;
use style::attr::AttrValue;

use crate::script::document_loader::{LoadBlocker, LoadType};
use crate::script::dom::attr::Attr;
use crate::script::dom::bindings::cell::DomRefCell;
use crate::script::dom::bindings::codegen::GenericBindings::HTMLXFrameBinding::HTMLXFrameMethods;
use crate::script::dom::bindings::inheritance::Castable;
use crate::script::dom::bindings::refcounted::Trusted;
use crate::script::dom::bindings::reflector::DomGlobal;
use crate::script::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::script::dom::bindings::str::{DOMString, USVString};
use crate::script::dom::document::Document;
use crate::script::dom::element::{AttributeMutation, Element};
use crate::script::dom::eventtarget::EventTarget;
use crate::script::dom::hpprpacket::HpprPacket;
use crate::script::dom::html::htmlelement::HTMLElement;
use crate::script::dom::node::{BindContext, Node, NodeDamage, NodeTraits, UnbindContext};
use crate::script::dom::virtualmethods::VirtualMethods;
use crate::script::dom::watchsocket::WatchSocket;
use crate::script::dom::windowproxy::WindowProxy;
use crate::script::script_runtime::CanGc;
use crate::script::script_thread::{ScriptThread, with_script_thread};
use crate::script::script_window_proxies::ScriptWindowProxies;

#[derive(PartialEq)]
enum PipelineType {
    InitialAboutBlank,
    Navigation,
}

#[derive(PartialEq)]
enum ProcessingMode {
    FirstTime,
    NotFirstTime,
}

#[derive(Clone, Copy, Debug, JSTraceable, MallocSizeOf, PartialEq, Eq)]
enum EmbedMode {
    Inherited,
    Isolated,
    Strict,
    SandboxPreview,
}

impl EmbedMode {
    fn as_str(self) -> &'static str {
        match self {
            EmbedMode::Inherited => "inherited",
            EmbedMode::Isolated => "isolated",
            EmbedMode::Strict => "strict",
            EmbedMode::SandboxPreview => "sandbox-preview",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmbedPolicy {
    Auto,
    Isolated,
    Strict,
}

#[dom_struct]
pub(crate) struct HTMLXFrame {
    htmlelement: HTMLElement,
    #[no_trace]
    webview_id: Cell<Option<WebViewId>>,
    #[no_trace]
    browsing_context_id: Cell<Option<BrowsingContextId>>,
    #[no_trace]
    pipeline_id: Cell<Option<PipelineId>>,
    #[no_trace]
    pending_pipeline_id: Cell<Option<PipelineId>>,
    #[no_trace]
    about_blank_pipeline_id: Cell<Option<PipelineId>>,
    load_blocker: DomRefCell<Option<LoadBlocker>>,
    throttled: Cell<bool>,
    #[conditional_malloc_size_of]
    script_window_proxies: Rc<ScriptWindowProxies>,
    /// Keeping track of whether the iframe will be navigated
    /// outside of the processing of it's attribute(for example: form navigation).
    /// This is necessary to prevent the iframe load event steps
    /// from asynchronously running for the initial blank document
    /// while script at this point(when the flag is set)
    /// expects those to run only for the navigated documented.
    pending_navigation: Cell<bool>,
    /// Current embed mode for the child browsing context.
    embed_mode: Cell<EmbedMode>,
    /// Resolved child content authority.
    content_authority: DomRefCell<Option<String>>,
    /// Monotonic embed resolve request id used to ignore stale preflight callbacks.
    embed_resolve_serial: Cell<u64>,
    /// Current watch prefix (None = not watching)
    watch_prefix: DomRefCell<Option<String>>,
    /// Reference to the shared WatchSocket (if watching)
    watch_socket: MutNullableDom<WatchSocket>,
}

impl HTMLXFrame {
    /// <https://html.spec.whatwg.org/multipage/#otherwise-steps-for-iframe-or-frame-elements>,
    /// step 1.
    fn get_url(&self) -> BrowserUrl {
        let element = self.upcast::<Element>();
        element
            .get_attribute(&ns!(), &local_name!("src"))
            .and_then(|src| {
                let url = src.value();
                if url.is_empty() {
                    None
                } else {
                    self.owner_document().base_url().join(&url).ok()
                }
            })
            .unwrap_or_else(|| BrowserUrl::parse("about:blank").unwrap())
    }

    fn policy(&self) -> EmbedPolicy {
        let policy = self.upcast::<Element>().get_string_attribute(&LocalName::from("policy"));
        match &*policy.str() {
            "isolated" => EmbedPolicy::Isolated,
            "strict" => EmbedPolicy::Strict,
            _ => EmbedPolicy::Auto,
        }
    }

    fn parent_content_authority(&self) -> Option<String> {
        self.owner_document().hppr_content_authority()
    }

    fn next_embed_resolve_serial(&self) -> u64 {
        let next = self.embed_resolve_serial.get().wrapping_add(1);
        self.embed_resolve_serial.set(next);
        next
    }

    fn set_embed_state(&self, mode: EmbedMode, content_authority: Option<String>) {
        self.embed_mode.set(mode);
        *self.content_authority.borrow_mut() = content_authority;
    }

    fn refresh_loaded_content_authority(&self) {
        let Some(pipeline_id) = self.pipeline_id.get() else {
            return;
        };
        let Some(document) = ScriptThread::find_document(pipeline_id) else {
            return;
        };
        if let Some(content_authority) = document.hppr_content_authority() {
            *self.content_authority.borrow_mut() = Some(content_authority);
        }
    }

    fn current_content_authority(&self) -> Option<String> {
        self.refresh_loaded_content_authority();
        self.content_authority.borrow().clone()
    }

    fn current_embed_mode(&self) -> EmbedMode {
        self.embed_mode.get()
    }

    fn is_hppr_embed_url(url: &BrowserUrl) -> bool {
        url.scheme() == "hppr"
    }

    fn is_sandbox_preview_url(url: &BrowserUrl) -> bool {
        url.scheme() == "hppr-sandbox"
    }

    fn compute_embed_mode(
        &self,
        url: &BrowserUrl,
        policy: EmbedPolicy,
        child_content_authority: Option<&str>,
    ) -> EmbedMode {
        if Self::is_sandbox_preview_url(url) {
            return EmbedMode::SandboxPreview;
        }
        if !Self::is_hppr_embed_url(url) {
            return EmbedMode::Inherited;
        }
        match policy {
            EmbedPolicy::Strict => EmbedMode::Strict,
            EmbedPolicy::Isolated => EmbedMode::Isolated,
            EmbedPolicy::Auto => {
                let Some(parent_authority) = self.parent_content_authority() else {
                    return EmbedMode::Isolated;
                };
                let Some(child_authority) = child_content_authority else {
                    return EmbedMode::Isolated;
                };
                if parent_authority == child_authority {
                    EmbedMode::Inherited
                } else {
                    EmbedMode::Isolated
                }
            },
        }
    }

    fn sandbox_flags_for_mode(mode: EmbedMode) -> SandboxingFlagSet {
        let navigation_flags =
            SandboxingFlagSet::SANDBOXED_AUXILIARY_NAVIGATION_BROWSING_CONTEXT_FLAG |
            SandboxingFlagSet::SANDBOXED_TOP_LEVEL_NAVIGATION_WITHOUT_USER_ACTIVATION_BROWSING_CONTEXT_FLAG |
            SandboxingFlagSet::SANDBOXED_TOP_LEVEL_NAVIGATION_WITH_USER_ACTIVATION_BROWSING_CONTEXT_FLAG;
        match mode {
            EmbedMode::Inherited | EmbedMode::SandboxPreview => SandboxingFlagSet::empty(),
            EmbedMode::Isolated => {
                SandboxingFlagSet::SANDBOXED_ORIGIN_BROWSING_CONTEXT_FLAG | navigation_flags
            },
            EmbedMode::Strict => {
                SandboxingFlagSet::SANDBOXED_ORIGIN_BROWSING_CONTEXT_FLAG |
                    SandboxingFlagSet::SANDBOXED_SCRIPTS_BROWSING_CONTEXT_FLAG |
                    SandboxingFlagSet::SANDBOXED_FORMS_BROWSING_CONTEXT_FLAG |
                    navigation_flags
            },
        }
    }

    pub(crate) fn navigate_or_reload_child_browsing_context(
        &self,
        load_data: LoadData,
        history_handling: NavigationHistoryBehavior,
    ) {
        self.pending_navigation.set(true);
        self.start_new_pipeline(load_data, PipelineType::Navigation, history_handling);
    }

    fn start_new_pipeline(
        &self,
        load_data: LoadData,
        pipeline_type: PipelineType,
        history_handling: NavigationHistoryBehavior,
    ) {
        let browsing_context_id = match self.browsing_context_id() {
            None => return warn!("Attempted to start a new pipeline on an unattached <x>."),
            Some(id) => id,
        };

        let webview_id = match self.webview_id() {
            None => return warn!("Attempted to start a new pipeline on an unattached <x>."),
            Some(id) => id,
        };

        let document = self.owner_document();

        {
            let load_blocker = &self.load_blocker;
            // Any outstanding subframe load is finished from the point of view of the blocked
            // document; the new navigation will continue blocking it.
            LoadBlocker::terminate_subframe(load_blocker);
        }

        match load_data.js_eval_result {
            Some(JsEvalResult::NoContent) => (),
            _ => {
                let mut load_blocker = self.load_blocker.borrow_mut();
                *load_blocker = Some(LoadBlocker::new(
                    &document,
                    LoadType::Subframe(load_data.url.clone()),
                ));
            },
        };

        let window = self.owner_window();
        let old_pipeline_id = self.pipeline_id();
        let new_pipeline_id = PipelineId::new();
        self.pending_pipeline_id.set(Some(new_pipeline_id));

        let load_info = IFrameLoadInfo {
            parent_pipeline_id: window.pipeline_id(),
            browsing_context_id,
            webview_id,
            new_pipeline_id,
            is_private: false, // FIXME
            inherited_secure_context: load_data.inherited_secure_context,
            history_handling,
        };

        let viewport_details = window
            .get_iframe_viewport_details_if_known(browsing_context_id)
            .unwrap_or_else(|| ViewportDetails {
                hidpi_scale_factor: window.device_pixel_ratio(),
                ..Default::default()
            });

        match pipeline_type {
            PipelineType::InitialAboutBlank => {
                self.about_blank_pipeline_id.set(Some(new_pipeline_id));

                let load_info = IFrameLoadInfoWithData {
                    info: load_info,
                    load_data: load_data.clone(),
                    old_pipeline_id,
                    viewport_details,
                    theme: window.theme(),
                };
                window
                    .as_global_scope()
                    .script_to_constellation_chan()
                    .send(ScriptToConstellationMessage::ScriptNewIFrame(load_info))
                    .unwrap();

                let new_pipeline_info = NewPipelineInfo {
                    parent_info: Some(window.pipeline_id()),
                    new_pipeline_id,
                    browsing_context_id,
                    webview_id,
                    opener: None,
                    load_data,
                    viewport_details,
                    user_content_manager_id: None,
                    theme: window.theme(),
                };

                self.pipeline_id.set(Some(new_pipeline_id));
                with_script_thread(|script_thread| {
                    script_thread.spawn_pipeline(new_pipeline_info);
                });
            },
            PipelineType::Navigation => {
                let load_info = IFrameLoadInfoWithData {
                    info: load_info,
                    load_data,
                    old_pipeline_id,
                    viewport_details,
                    theme: window.theme(),
                };
                window
                    .as_global_scope()
                    .script_to_constellation_chan()
                    .send(ScriptToConstellationMessage::ScriptLoadedURLInIFrame(
                        load_info,
                    ))
                    .unwrap();
            },
        }
    }

    /// When an <x> is first inserted into the document,
    /// an "about:blank" document is created,
    /// and synchronously processed by the script thread.
    /// This initial synchronous load should have no noticeable effect in script.
    pub(crate) fn is_initial_blank_document(&self) -> bool {
        self.pending_pipeline_id.get() == self.about_blank_pipeline_id.get()
    }

    fn navigate_with_embed_mode(
        &self,
        url: BrowserUrl,
        embed_mode: EmbedMode,
        content_authority: Option<String>,
    ) {
        let window = self.owner_window();
        let document = self.owner_document();

        self.set_embed_state(embed_mode, content_authority);

        let creator_pipeline_id = if url.as_str() == "about:blank" {
            Some(window.pipeline_id())
        } else {
            None
        };

        let propagate_encoding_to_child_document = url.origin().same_origin(window.origin());
        let mut load_data = LoadData::new(
            LoadOrigin::Script(document.origin().snapshot()),
            url,
            None,
            creator_pipeline_id,
            window.as_global_scope().get_referrer(),
            document.get_referrer_policy(),
            Some(window.as_global_scope().is_secure_context()),
            Some(document.insecure_requests_policy()),
            document.has_trustworthy_ancestor_or_current_origin(),
            Self::sandbox_flags_for_mode(embed_mode),
        );
        load_data.destination = Destination::IFrame;
        load_data.policy_container = Some(window.as_global_scope().policy_container());
        if propagate_encoding_to_child_document {
            load_data.container_document_encoding = Some(document.encoding());
        }

        let pipeline_id = self.pipeline_id();
        let is_about_blank =
            pipeline_id.is_some() && pipeline_id == self.about_blank_pipeline_id.get();
        let history_handling = if is_about_blank {
            NavigationHistoryBehavior::Replace
        } else {
            NavigationHistoryBehavior::Push
        };

        self.navigate_or_reload_child_browsing_context(load_data, history_handling);
    }

    fn start_embed_preflight(&self, url: BrowserUrl, request_serial: u64) {
        let task_source = self
            .owner_window()
            .as_global_scope()
            .task_manager()
            .dom_manipulation_task_source()
            .to_sendable();
        let trusted_xframe = Trusted::new(self);
        let callback_url = url.clone();
        let requested_url = url.to_string();
        let callback = GenericCallback::new(move |message| {
            let trusted_xframe = trusted_xframe.clone();
            let callback_url = callback_url.clone();
            task_source.queue(task!(xframe_embed_resolve: move || {
                let xframe = trusted_xframe.root();
                if xframe.embed_resolve_serial.get() != request_serial {
                    return;
                }
                let content_authority = match message {
                    Ok(HpprControlResponse::EmbedResolve(HpprEmbedResolveResponse {
                        content_authority,
                    })) => content_authority,
                    Ok(HpprControlResponse::Error(error)) => {
                        warn!("<x> embed resolve failed for {}: {}", callback_url, error);
                        None
                    },
                    Ok(other) => {
                        warn!(
                            "<x> unexpected embed resolve response for {}: {:?}",
                            callback_url,
                            other
                        );
                        None
                    },
                    Err(error) => {
                        warn!("<x> embed resolve callback failed for {}: {}", callback_url, error);
                        None
                    },
                };

                let embed_mode = xframe.compute_embed_mode(
                    &callback_url,
                    EmbedPolicy::Auto,
                    content_authority.as_deref(),
                );
                if !xframe.upcast::<Node>().is_connected_with_browsing_context() {
                    return;
                }
                xframe.navigate_with_embed_mode(callback_url, embed_mode, content_authority);
            }));
        })
        .expect("Could not create <x> embed resolve callback");

        self.owner_window().send_to_embedder(EmbedderMsg::HpprControlOperation(
            self.owner_window().webview_id(),
            self.owner_window().as_global_scope().get_url().to_string(),
            HpprControlRequest::EmbedResolve {
                url: requested_url,
            },
            callback,
        ));
    }

    /// Process the <x> attributes
    fn process_the_iframe_attributes(&self, mode: ProcessingMode) {
        if mode == ProcessingMode::FirstTime &&
            !self.upcast::<Element>().has_attribute(&local_name!("src"))
        {
            return;
        }

        let url = self.get_url();
        let policy = self.policy();
        let request_serial = self.next_embed_resolve_serial();
        self.pending_navigation.set(true);

        if Self::is_hppr_embed_url(&url) && policy == EmbedPolicy::Auto {
            self.start_embed_preflight(url, request_serial);
            return;
        }

        let embed_mode = self.compute_embed_mode(&url, policy, None);
        self.navigate_with_embed_mode(url, embed_mode, None);
    }

    /// Create a new child navigable for <x>
    /// Synchronously create a new browsing context (This is not a navigation).
    fn create_nested_browsing_context(&self) {
        let url = BrowserUrl::parse("about:blank").unwrap();
        let document = self.owner_document();
        let window = self.owner_window();
        let pipeline_id = Some(window.pipeline_id());
        let mut load_data = LoadData::new(
            LoadOrigin::Script(document.origin().snapshot()),
            url,
            None,
            pipeline_id,
            window.as_global_scope().get_referrer(),
            document.get_referrer_policy(),
            Some(window.as_global_scope().is_secure_context()),
            Some(document.insecure_requests_policy()),
            document.has_trustworthy_ancestor_or_current_origin(),
            SandboxingFlagSet::empty(),
        );
        load_data.destination = Destination::IFrame;
        load_data.policy_container = Some(window.as_global_scope().policy_container());

        let browsing_context_id = BrowsingContextId::new();
        let webview_id = window.window_proxy().webview_id();
        self.pipeline_id.set(None);
        self.pending_pipeline_id.set(None);
        self.webview_id.set(Some(webview_id));
        self.browsing_context_id.set(Some(browsing_context_id));
        self.start_new_pipeline(
            load_data,
            PipelineType::InitialAboutBlank,
            NavigationHistoryBehavior::Push,
        );
    }

    fn destroy_nested_browsing_context(&self) {
        self.pipeline_id.set(None);
        self.pending_pipeline_id.set(None);
        self.about_blank_pipeline_id.set(None);
        self.webview_id.set(None);
        self.browsing_context_id.set(None);
    }

    pub(crate) fn update_pipeline_id(
        &self,
        new_pipeline_id: PipelineId,
        reason: UpdatePipelineIdReason,
        cx: &mut js::context::JSContext,
    ) {
        // For all updates except the one for the initial blank document,
        // we need to set the flag back to false because the navigation is complete.
        if !self.is_initial_blank_document() {
            self.pending_navigation.set(false);
        }
        if self.pending_pipeline_id.get() != Some(new_pipeline_id) &&
            reason == UpdatePipelineIdReason::Navigation
        {
            return;
        }

        self.pipeline_id.set(Some(new_pipeline_id));

        // Only terminate the load blocker if the pipeline id was updated due to a traversal.
        // The load blocker will be terminated for a navigation in iframe_load_event_steps.
        if reason == UpdatePipelineIdReason::Traversal {
            let blocker = &self.load_blocker;
            LoadBlocker::terminate(blocker, cx);
        }

        self.upcast::<Node>().dirty(NodeDamage::Other);
    }

    fn new_inherited(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> HTMLXFrame {
        HTMLXFrame {
            htmlelement: HTMLElement::new_inherited(local_name, prefix, document),
            browsing_context_id: Cell::new(None),
            webview_id: Cell::new(None),
            pipeline_id: Cell::new(None),
            pending_pipeline_id: Cell::new(None),
            about_blank_pipeline_id: Cell::new(None),
            load_blocker: DomRefCell::new(None),
            throttled: Cell::new(false),
            script_window_proxies: ScriptThread::window_proxies(),
            pending_navigation: Default::default(),
            embed_mode: Cell::new(EmbedMode::Inherited),
            content_authority: DomRefCell::new(None),
            embed_resolve_serial: Cell::new(0),
            watch_prefix: DomRefCell::new(None),
            watch_socket: Default::default(),
        }
    }

    pub(crate) fn new(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
        proto: Option<HandleObject>,
        can_gc: CanGc,
    ) -> DomRoot<HTMLXFrame> {
        Node::reflect_node_with_proto(
            Box::new(HTMLXFrame::new_inherited(
                local_name, prefix, document,
            )),
            document,
            proto,
            can_gc,
        )
    }

    #[inline]
    pub(crate) fn pipeline_id(&self) -> Option<PipelineId> {
        self.pipeline_id.get()
    }

    #[inline]
    pub(crate) fn browsing_context_id(&self) -> Option<BrowsingContextId> {
        self.browsing_context_id.get()
    }

    #[inline]
    pub(crate) fn webview_id(&self) -> Option<WebViewId> {
        self.webview_id.get()
    }

    pub(crate) fn set_throttled(&self, throttled: bool) {
        if self.throttled.get() != throttled {
            self.throttled.set(throttled);
        }
    }

    /// Load event steps for <x>
    pub(crate) fn iframe_load_event_steps(&self, loaded_pipeline: PipelineId, cx: &mut js::context::JSContext) {
        if Some(loaded_pipeline) != self.pending_pipeline_id.get() {
            return;
        }

        let should_fire_event = if self.is_initial_blank_document() {
            // If this is the initial blank doc:
            // do not fire if there is a pending navigation,
            // or if the element has an src.
            !self.pending_navigation.get() &&
                !self.upcast::<Element>().has_attribute(&local_name!("src"))
        } else {
            // If this is not the initial blank doc:
            // do not fire if there is a pending navigation.
            !self.pending_navigation.get()
        };
        self.refresh_loaded_content_authority();
        if should_fire_event {
            self.upcast::<EventTarget>()
                .fire_event(atom!("load"), CanGc::from_cx(cx));
        }

        let blocker = &self.load_blocker;
        LoadBlocker::terminate(blocker, cx);
    }

    /// Destroy document and its descendants
    pub(crate) fn destroy_document_and_its_descendants(&self, cx: &mut js::context::JSContext) {
        let Some(pipeline_id) = self.pipeline_id.get() else {
            return;
        };
        if let Some(exited_document) = ScriptThread::find_document(pipeline_id) {
            exited_document.destroy_document_and_its_descendants(cx);
        }
        self.destroy_nested_browsing_context();
    }

    /// Destroy child navigable
    fn destroy_child_navigable(&self, cx: &mut JSContext) {
        let blocker = &self.load_blocker;
        LoadBlocker::terminate(blocker, cx);

        let Some(browsing_context_id) = self.browsing_context_id() else {
            return;
        };
        let pipeline_id = self.pipeline_id.get();

        self.destroy_nested_browsing_context();

        let (sender, receiver) =
            ProfiledIpc::channel(self.global().time_profiler_chan().clone()).unwrap();
        let msg = ScriptToConstellationMessage::RemoveIFrame(browsing_context_id, sender);
        self.owner_window()
            .as_global_scope()
            .script_to_constellation_chan()
            .send(msg)
            .unwrap();
        let _exited_pipeline_ids = receiver.recv().unwrap();
        let Some(pipeline_id) = pipeline_id else {
            return;
        };
        if let Some(exited_document) = ScriptThread::find_document(pipeline_id) {
            exited_document.destroy_document_and_its_descendants(cx);
        }
    }

    /// Compute the watch prefix for the current src.
    /// Returns None if src is not an HPPR coordinate or watch is not set.
    fn compute_watch_prefix(&self) -> Option<String> {
        let element = self.upcast::<Element>();
        if !element.has_attribute(&LocalName::from("watch")) {
            return None;
        }
        // Explicit watch value overrides default
        let watch_attr = element.get_attribute(&ns!(), &LocalName::from("watch"));
        if let Some(ref attr) = watch_attr {
            let val = attr.value();
            let val = val.as_ref() as &str;
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
        // Default: derive //group/app/ from resolved src URL
        let url = self.get_url();
        if url.scheme() != "hppr" {
            return None;
        }
        let path = url.path();
        let urc = hppr_packet::urc::URC::parse(path.to_string()).ok()?;
        let parts = urc.parts();
        if parts.group.is_empty() || parts.app.is_empty() {
            return None;
        }
        Some(format!("//{}/{}/", parts.group, parts.app))
    }

    /// Start or update watching based on current attributes.
    fn update_watch(&self, cx: &mut JSContext) {
        let can_gc = CanGc::from_cx(cx);
        let new_prefix = self.compute_watch_prefix();
        let old_prefix = self.watch_prefix.borrow().clone();

        if old_prefix == new_prefix {
            return;
        }

        // Release old watch
        if let Some(ref old) = old_prefix {
            if let Some(ws) = self.watch_socket.get() {
                ws.remove_watch_subscriber(self);
            }
            self.owner_document().release_watch(old);
            self.watch_socket.set(None);
        }

        // Acquire new watch
        if let Some(ref prefix) = new_prefix {
            if let Some(ws) = self.owner_document().acquire_watch(prefix, can_gc) {
                ws.add_watch_subscriber(self);
                self.watch_socket.set(Some(&ws));
            }
        }

        *self.watch_prefix.borrow_mut() = new_prefix;
    }

    /// Stop watching entirely.
    fn stop_watch(&self) {
        if let Some(ref prefix) = self.watch_prefix.borrow().clone() {
            if let Some(ws) = self.watch_socket.get() {
                ws.remove_watch_subscriber(self);
            }
            self.owner_document().release_watch(prefix);
            self.watch_socket.set(None);
        }
        *self.watch_prefix.borrow_mut() = None;
    }

    /// Called by WatchSocket when a watch message arrives.
    pub(crate) fn on_watch_message(&self, data: &str, _cx: &mut JSContext) {
        let (op, coord) = match data.split_once(' ') {
            Some((op, coord)) => (op, coord),
            None => return,
        };
        if op != "+" {
            return;
        }
        let url = self.get_url();
        if url.scheme() != "hppr" {
            return;
        }
        let path = url.path();
        let our_loc = path.trim_end_matches('/');
        let coord_base = coord.split("/|/").next().unwrap_or(coord);
        let coord_base = coord_base.trim_end_matches('/');
        if coord_base == our_loc || coord_base.starts_with(&format!("{}/", our_loc)) {
            self.process_the_iframe_attributes(ProcessingMode::NotFirstTime);
        }
    }

    /// Called by WatchSocket on connection error.
    pub(crate) fn on_watch_error(&self, cx: &mut JSContext) {
        self.upcast::<EventTarget>().fire_event(atom!("error"), CanGc::from_cx(cx));
    }
}

impl HTMLXFrameMethods<crate::DomTypeHolder> for HTMLXFrame {
    // https://html.spec.whatwg.org/multipage/#dom-iframe-src
    make_url_getter!(Src, "src");

    // https://html.spec.whatwg.org/multipage/#dom-iframe-src
    make_url_setter!(SetSrc, "src");

    /// Get the content window
    fn GetContentWindow(&self) -> Option<DomRoot<WindowProxy>> {
        self.browsing_context_id
            .get()
            .and_then(|id| self.script_window_proxies.find_window_proxy(id))
    }

    /// Get the content document
    fn GetContentDocument(&self) -> Option<DomRoot<Document>> {
        if self.current_embed_mode() != EmbedMode::Inherited {
            return None;
        }
        let pipeline_id = self.pipeline_id.get()?;
        let document = ScriptThread::find_document(pipeline_id)?;
        if !self
            .owner_document()
            .origin()
            .same_origin_domain(document.origin())
        {
            return None;
        }
        Some(document)
    }

    // Width attribute
    make_getter!(Width, "width");
    make_dimension_setter!(SetWidth, "width");

    // Height attribute
    make_getter!(Height, "height");
    make_dimension_setter!(SetHeight, "height");

    /// Access the nested document's HPPR packet
    fn GetPacket(&self) -> Option<DomRoot<HpprPacket>> {
        let pipeline_id = self.pipeline_id.get()?;
        let document = ScriptThread::find_document(pipeline_id)?;
        document.hppr_packet()
    }

    fn Policy(&self) -> DOMString {
        let element = self.upcast::<Element>();
        element.get_string_attribute(&LocalName::from("policy"))
    }

    fn SetPolicy(&self, value: DOMString) {
        let element = self.upcast::<Element>();
        element.set_string_attribute(&LocalName::from("policy"), value, CanGc::note())
    }

    fn GetContentAuthority(&self) -> Option<DOMString> {
        self.current_content_authority().map(DOMString::from)
    }

    fn EmbedMode(&self) -> DOMString {
        DOMString::from(self.current_embed_mode().as_str())
    }

    // Watch attribute
    fn Watch(&self) -> DOMString {
        let element = self.upcast::<Element>();
        element.get_string_attribute(&LocalName::from("watch"))
    }
    fn SetWatch(&self, value: DOMString) {
        let element = self.upcast::<Element>();
        element.set_string_attribute(&LocalName::from("watch"), value, CanGc::note())
    }
}

impl VirtualMethods for HTMLXFrame {
    fn super_type(&self) -> Option<&dyn VirtualMethods> {
        Some(self.upcast::<HTMLElement>() as &dyn VirtualMethods)
    }

    #[expect(unsafe_code)]
    fn attribute_mutated(&self, attr: &Attr, mutation: AttributeMutation, _can_gc: CanGc) {
        let mut cx = unsafe { script_bindings::script_runtime::temp_cx() };
        let cx = &mut cx;
        self.super_type()
            .unwrap()
            .attribute_mutated(attr, mutation, CanGc::from_cx(cx));
        match *attr.local_name() {
            local_name!("src") => {
                // When src attribute is set, changed, or removed,
                // process the <x> attributes.
                if self.upcast::<Node>().is_connected_with_browsing_context() {
                    debug!("<x> src set while in browsing context.");
                    self.process_the_iframe_attributes(ProcessingMode::NotFirstTime);
                    self.update_watch(cx);
                }
            },
            ref name if *name == LocalName::from("policy") => {
                if self.upcast::<Node>().is_connected_with_browsing_context() {
                    self.process_the_iframe_attributes(ProcessingMode::NotFirstTime);
                }
            },
            ref name if *name == LocalName::from("watch") => {
                if self.upcast::<Node>().is_connected_with_browsing_context() {
                    self.update_watch(cx);
                }
            },
            _ => {},
        }
    }

    fn attribute_affects_presentational_hints(&self, attr: &Attr) -> bool {
        match attr.local_name() {
            &local_name!("width") | &local_name!("height") => true,
            _ => self
                .super_type()
                .unwrap()
                .attribute_affects_presentational_hints(attr),
        }
    }

    fn parse_plain_attribute(&self, name: &LocalName, value: DOMString) -> AttrValue {
        match *name {
            local_name!("width") => AttrValue::from_dimension(value.into()),
            local_name!("height") => AttrValue::from_dimension(value.into()),
            _ => self
                .super_type()
                .unwrap()
                .parse_plain_attribute(name, value),
        }
    }

    /// Post connection steps for <x> element
    fn post_connection_steps(&self, cx: &mut JSContext) {
        if let Some(s) = self.super_type() {
            s.post_connection_steps(cx);
        }
        if !self.upcast::<Node>().is_connected_with_browsing_context() {
            return;
        }

        debug!("<<x>> running post connection steps");

        // Create a new child navigable
        self.create_nested_browsing_context();

        // Process the <x> attributes
        self.process_the_iframe_attributes(ProcessingMode::FirstTime);
        self.update_watch(cx);
    }

    fn bind_to_tree(&self, context: &BindContext, can_gc: CanGc) {
        if let Some(s) = self.super_type() {
            s.bind_to_tree(context, can_gc);
        }
        self.owner_document().invalidate_iframes_collection();
    }

    /// Removing steps for <x> element
    #[expect(unsafe_code)]
    fn unbind_from_tree(&self, context: &UnbindContext, can_gc: CanGc) {
        self.super_type().unwrap().unbind_from_tree(context, can_gc);

        // Stop watching before destroying navigable
        self.stop_watch();

        // Destroy the child navigable
        let mut cx = unsafe { script_bindings::script_runtime::temp_cx() };
        self.destroy_child_navigable(&mut cx);

        self.owner_document().invalidate_iframes_collection();
    }
}
