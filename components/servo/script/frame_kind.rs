/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Abstraction over navigable frame elements (iframe, x).
//!
//! Both HTMLIFrameElement and HTMLXFrame implement the same navigable-frame
//! interface. This module provides FrameKind to dispatch between them without
//! modifying upstream Servo collection logic.

use base::id::{BrowsingContextId, PipelineId};
use crate::constellation::{LoadData, NavigationHistoryBehavior};
use crate::script::UpdatePipelineIdReason;

use crate::script::dom::bindings::codegen::GenericBindings::HTMLIFrameElementBinding::HTMLIFrameElementMethods;
use crate::script::dom::bindings::codegen::GenericBindings::HTMLXFrameBinding::HTMLXFrameMethods;
use crate::script::dom::bindings::inheritance::Castable;
use crate::script::dom::bindings::root::{Dom, DomRoot};
use crate::script::dom::document::Document;
use crate::script::dom::element::Element;
use crate::script::dom::html::htmliframeelement::HTMLIFrameElement;
use crate::script::dom::html::htmlxframe::HTMLXFrame;
use crate::script::dom::node::Node;
use crate::script::dom::windowproxy::WindowProxy;

/// A navigable frame element — either an `<iframe>` or an `<x>`.
#[derive(Clone, JSTraceable, MallocSizeOf)]
#[cfg_attr(crown, crown::unrooted_must_root_lint::must_root)]
pub(crate) enum FrameKind {
    IFrame(Dom<HTMLIFrameElement>),
    XFrame(Dom<HTMLXFrame>),
}

impl FrameKind {
    pub fn browsing_context_id(&self) -> Option<BrowsingContextId> {
        match self {
            Self::IFrame(e) => e.browsing_context_id(),
            Self::XFrame(e) => e.browsing_context_id(),
        }
    }

    pub fn pipeline_id(&self) -> Option<PipelineId> {
        match self {
            Self::IFrame(e) => e.pipeline_id(),
            Self::XFrame(e) => e.pipeline_id(),
        }
    }

    pub fn set_throttled(&self, throttled: bool) {
        match self {
            Self::IFrame(e) => e.set_throttled(throttled),
            Self::XFrame(e) => e.set_throttled(throttled),
        }
    }

    pub fn update_pipeline_id(
        &self,
        new_pipeline_id: PipelineId,
        reason: UpdatePipelineIdReason,
        cx: &mut js::context::JSContext,
    ) {
        match self {
            Self::IFrame(e) => e.update_pipeline_id(new_pipeline_id, reason, cx),
            Self::XFrame(e) => e.update_pipeline_id(new_pipeline_id, reason, cx),
        }
    }

    pub fn iframe_load_event_steps(&self, loaded_pipeline: PipelineId, cx: &mut js::context::JSContext) {
        match self {
            Self::IFrame(e) => e.iframe_load_event_steps(loaded_pipeline, cx),
            Self::XFrame(e) => e.iframe_load_event_steps(loaded_pipeline, cx),
        }
    }

    pub fn navigate_or_reload_child_browsing_context(
        &self,
        load_data: LoadData,
        history_handling: NavigationHistoryBehavior,
        cx: &mut js::context::JSContext,
    ) {
        match self {
            Self::IFrame(e) => {
                e.navigate_or_reload_child_browsing_context(load_data, history_handling, cx)
            },
            Self::XFrame(e) => {
                let _ = cx;
                e.navigate_or_reload_child_browsing_context(load_data, history_handling)
            },
        }
    }

    pub fn destroy_document_and_its_descendants(&self, cx: &mut js::context::JSContext) {
        match self {
            Self::IFrame(e) => e.destroy_document_and_its_descendants(cx),
            Self::XFrame(e) => e.destroy_document_and_its_descendants(cx),
        }
    }

    pub fn get_content_document(&self) -> Option<DomRoot<Document>> {
        match self {
            Self::IFrame(e) => e.GetContentDocument(),
            Self::XFrame(e) => e.GetContentDocument(),
        }
    }

    pub fn get_content_window(&self) -> Option<DomRoot<WindowProxy>> {
        match self {
            Self::IFrame(e) => e.GetContentWindow(),
            Self::XFrame(e) => e.GetContentWindow(),
        }
    }

    pub fn upcast_element(&self) -> DomRoot<Element> {
        match self {
            Self::IFrame(e) => DomRoot::upcast(e.as_rooted()),
            Self::XFrame(e) => DomRoot::upcast(e.as_rooted()),
        }
    }

    pub fn owner_document(&self) -> DomRoot<Document> {
        match self {
            Self::IFrame(e) => e.upcast::<Node>().owner_doc(),
            Self::XFrame(e) => e.upcast::<Node>().owner_doc(),
        }
    }

    pub fn as_iframe(&self) -> Option<DomRoot<HTMLIFrameElement>> {
        match self {
            Self::IFrame(e) => Some(e.as_rooted()),
            Self::XFrame(_) => None,
        }
    }
}
