//! Render-facing adapter for Servo layout fragment semantics.
//!
//! The render pipeline consumes layout's fragment tree directly and preserves
//! the fragment variants that affect CSS paint ordering. In particular,
//! `AbsoluteOrFixedPositioned` placeholders remain visible until stacking
//! contexts and final paint items have been built.
//!
//! Architecture boundary:
//! - layout owns fragment semantics and hoisting rules
//! - render reads those semantics without flattening them into a simplified tree
//! - leaf payload extraction may still use `havi_types` conversion helpers where
//!   Makepad draw code needs concrete text/image/iframe data
//!
//! This module is the render-side entry point for semantic fragment traversal.

use std::sync::Arc;

use base::id::WebViewId;
use havi_types::Fragment;
use layout_api::{shared_fragment_tree_for, SharedFragmentTree};

/// The current shared fragment source used by havi-render.
///
/// Today this still reads the shared fragment registry, but the active semantic
/// render path now treats the adapter as the only entry point rather than
/// depending on converted payload ownership at the call site.
#[derive(Clone)]
pub(crate) struct LayoutFragmentSource {
    shared_fragments: SharedFragmentTree,
}

impl LayoutFragmentSource {
    pub(crate) fn new(webview_id: WebViewId) -> Self {
        Self {
            shared_fragments: shared_fragment_tree_for(webview_id),
        }
    }

    pub(crate) fn fragments_arc(&self) -> Option<Arc<Vec<Fragment>>> {
        self.shared_fragments.get()
    }
}
