/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Abstraction over navigable frame elements (iframe, x).
//!
//! Both HTMLIFrameElement and HTMLXFrame implement the same navigable-frame
//! interface. This module provides FrameKind to dispatch between them without
//! modifying upstream Servo collection logic.

use base::id::{BrowsingContextId, PipelineId};
use constellation_traits::{LoadData, NavigationHistoryBehavior};
use script_bindings::script_runtime::CanGc;
use script_traits::UpdatePipelineIdReason;

use crate::dom::bindings::codegen::Bindings::HTMLIFrameElementBinding::HTMLIFrameElementMethods;
use crate::dom::bindings::codegen::Bindings::HTMLXFrameBinding::HTMLXFrameMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::root::{Dom, DomRoot};
use crate::dom::document::Document;
use crate::dom::element::Element;
use crate::dom::html::htmliframeelement::HTMLIFrameElement;
use crate::dom::html::htmlxframe::HTMLXFrame;
use crate::dom::node::Node;
use crate::dom::windowproxy::WindowProxy;

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
        can_gc: CanGc,
    ) {
        match self {
            Self::IFrame(e) => e.update_pipeline_id(new_pipeline_id, reason, can_gc),
            Self::XFrame(e) => e.update_pipeline_id(new_pipeline_id, reason, can_gc),
        }
    }

    pub fn iframe_load_event_steps(&self, loaded_pipeline: PipelineId, can_gc: CanGc) {
        match self {
            Self::IFrame(e) => e.iframe_load_event_steps(loaded_pipeline, can_gc),
            Self::XFrame(e) => e.iframe_load_event_steps(loaded_pipeline, can_gc),
        }
    }

    pub fn navigate_or_reload_child_browsing_context(
        &self,
        load_data: LoadData,
        history_handling: NavigationHistoryBehavior,
        can_gc: CanGc,
    ) {
        match self {
            Self::IFrame(e) => {
                e.navigate_or_reload_child_browsing_context(load_data, history_handling, can_gc)
            },
            Self::XFrame(e) => {
                e.navigate_or_reload_child_browsing_context(load_data, history_handling, can_gc)
            },
        }
    }

    pub fn destroy_document_and_its_descendants(&self, can_gc: CanGc) {
        match self {
            Self::IFrame(e) => e.destroy_document_and_its_descendants(can_gc),
            Self::XFrame(e) => e.destroy_document_and_its_descendants(can_gc),
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
