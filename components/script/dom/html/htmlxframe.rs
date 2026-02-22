/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;
use std::rc::Rc;

use base::id::{BrowsingContextId, PipelineId, WebViewId};
use constellation_traits::{
    IFrameLoadInfo, IFrameLoadInfoWithData, JsEvalResult, LoadData, LoadOrigin,
    NavigationHistoryBehavior, ScriptToConstellationMessage,
};
use content_security_policy::sandboxing_directive::SandboxingFlagSet;
use dom_struct::dom_struct;
use embedder_traits::ViewportDetails;
use html5ever::{LocalName, Prefix, local_name, ns};
use js::context::JSContext;
use js::rust::HandleObject;
use net_traits::request::Destination;
use profile_traits::ipc as ProfiledIpc;
use script_traits::{NewPipelineInfo, UpdatePipelineIdReason};
use servo_url::BrowserUrl;
use style::attr::AttrValue;

use crate::document_loader::{LoadBlocker, LoadType};
use crate::dom::attr::Attr;
use crate::dom::bindings::cell::DomRefCell;
use crate::dom::bindings::codegen::Bindings::HTMLXFrameBinding::HTMLXFrameMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::DomGlobal;
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::bindings::str::{DOMString, USVString};
use crate::dom::document::Document;
use crate::dom::element::{AttributeMutation, Element};
use crate::dom::eventtarget::EventTarget;
use crate::dom::hpprpacket::HpprPacket;
use crate::dom::html::htmlelement::HTMLElement;
use crate::dom::node::{BindContext, Node, NodeDamage, NodeTraits, UnbindContext};
use crate::dom::virtualmethods::VirtualMethods;
use crate::dom::watchsocket::WatchSocket;
use crate::dom::windowproxy::WindowProxy;
use crate::script_runtime::CanGc;
use crate::script_thread::{ScriptThread, with_script_thread};
use crate::script_window_proxies::ScriptWindowProxies;

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
    /// Whether this <x> element trusts its parent document
    trust_parent: Cell<bool>,
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

    pub(crate) fn navigate_or_reload_child_browsing_context(
        &self,
        load_data: LoadData,
        history_handling: NavigationHistoryBehavior,
        can_gc: CanGc,
    ) {
        self.start_new_pipeline(
            load_data,
            PipelineType::Navigation,
            history_handling,
            can_gc,
        );
    }

    fn start_new_pipeline(
        &self,
        load_data: LoadData,
        pipeline_type: PipelineType,
        history_handling: NavigationHistoryBehavior,
        can_gc: CanGc,
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
            // Any oustanding load is finished from the point of view of the blocked
            // document; the new navigation will continue blocking it.
            LoadBlocker::terminate(load_blocker, can_gc);
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

    /// Process the <x> attributes
    fn process_the_iframe_attributes(&self, mode: ProcessingMode, can_gc: CanGc) {
        let window = self.owner_window();

        if mode == ProcessingMode::FirstTime &&
            !self.upcast::<Element>().has_attribute(&local_name!("src"))
        {
            return;
        }

        // Get the URL from src attribute
        let url = self.get_url();

        let document = self.owner_document();

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
            SandboxingFlagSet::empty(),
        );
        load_data.destination = Destination::IFrame;
        load_data.policy_container = Some(window.as_global_scope().policy_container());
        if propagate_encoding_to_child_document {
            load_data.container_document_encoding = Some(document.encoding());
        }

        let pipeline_id = self.pipeline_id();
        // If the initial `about:blank` page is the current page, load with replacement enabled
        let is_about_blank =
            pipeline_id.is_some() && pipeline_id == self.about_blank_pipeline_id.get();

        let history_handling = if is_about_blank {
            NavigationHistoryBehavior::Replace
        } else {
            NavigationHistoryBehavior::Push
        };

        self.navigate_or_reload_child_browsing_context(load_data, history_handling, can_gc);
    }

    /// Create a new child navigable for <x>
    /// Synchronously create a new browsing context (This is not a navigation).
    fn create_nested_browsing_context(&self, can_gc: CanGc) {
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
            can_gc,
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
        can_gc: CanGc,
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
            LoadBlocker::terminate(blocker, can_gc);
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
            trust_parent: Cell::new(false),
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
    pub(crate) fn iframe_load_event_steps(&self, loaded_pipeline: PipelineId, can_gc: CanGc) {
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
        if should_fire_event {
            self.upcast::<EventTarget>()
                .fire_event(atom!("load"), can_gc);
        }

        let blocker = &self.load_blocker;
        LoadBlocker::terminate(blocker, can_gc);
    }

    /// Destroy document and its descendants
    pub(crate) fn destroy_document_and_its_descendants(&self, can_gc: CanGc) {
        let Some(pipeline_id) = self.pipeline_id.get() else {
            return;
        };
        if let Some(exited_document) = ScriptThread::find_document(pipeline_id) {
            exited_document.destroy_document_and_its_descendants(can_gc);
        }
        self.destroy_nested_browsing_context();
    }

    /// Destroy child navigable
    fn destroy_child_navigable(&self, can_gc: CanGc) {
        let blocker = &self.load_blocker;
        LoadBlocker::terminate(blocker, CanGc::note());

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
            exited_document.destroy_document_and_its_descendants(can_gc);
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
    fn update_watch(&self, can_gc: CanGc) {
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
    pub(crate) fn on_watch_message(&self, data: &str, can_gc: CanGc) {
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
            self.process_the_iframe_attributes(ProcessingMode::NotFirstTime, can_gc);
        }
    }

    /// Called by WatchSocket on connection error.
    pub(crate) fn on_watch_error(&self, can_gc: CanGc) {
        self.upcast::<EventTarget>().fire_event(atom!("error"), can_gc);
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

    /// trustParent attribute getter
    fn TrustParent(&self) -> bool {
        self.trust_parent.get()
    }

    /// trustParent attribute setter
    fn SetTrustParent(&self, value: bool) {
        self.trust_parent.set(value);
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

    fn attribute_mutated(&self, attr: &Attr, mutation: AttributeMutation, can_gc: CanGc) {
        self.super_type()
            .unwrap()
            .attribute_mutated(attr, mutation, can_gc);
        match *attr.local_name() {
            local_name!("src") => {
                // When src attribute is set, changed, or removed,
                // process the <x> attributes.
                if self.upcast::<Node>().is_connected_with_browsing_context() {
                    debug!("<x> src set while in browsing context.");
                    self.process_the_iframe_attributes(ProcessingMode::NotFirstTime, can_gc);
                    self.update_watch(can_gc);
                }
            },
            ref name if *name == LocalName::from("watch") => {
                if self.upcast::<Node>().is_connected_with_browsing_context() {
                    self.update_watch(can_gc);
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
        let can_gc = CanGc::from_cx(cx);

        if !self.upcast::<Node>().is_connected_with_browsing_context() {
            return;
        }

        debug!("<<x>> running post connection steps");

        // Create a new child navigable
        self.create_nested_browsing_context(can_gc);

        // Process the <x> attributes
        self.process_the_iframe_attributes(ProcessingMode::FirstTime, can_gc);
        self.update_watch(can_gc);
    }

    fn bind_to_tree(&self, context: &BindContext, can_gc: CanGc) {
        if let Some(s) = self.super_type() {
            s.bind_to_tree(context, can_gc);
        }
        self.owner_document().invalidate_iframes_collection();
    }

    /// Removing steps for <x> element
    fn unbind_from_tree(&self, context: &UnbindContext, can_gc: CanGc) {
        self.super_type().unwrap().unbind_from_tree(context, can_gc);

        // Stop watching before destroying navigable
        self.stop_watch();

        // Destroy the child navigable
        self.destroy_child_navigable(can_gc);

        self.owner_document().invalidate_iframes_collection();
    }
}
