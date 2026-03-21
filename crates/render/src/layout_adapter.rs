//! Render-facing adapter for semantic fragment traversal.
//!
//! The active render path reads the semantic fragment tree from the shared
//! layout publication boundary using `havi-fragment-semantics`.

use std::sync::Arc;

use base::id::WebViewId;
use havi_fragment_semantics::Fragment;
use layout_api::{shared_layout_fragment_tree_for, SharedLayoutFragmentTree};

pub(crate) type LayoutFragmentTree = Arc<Vec<Fragment>>;

#[derive(Clone)]
pub(crate) struct LayoutFragmentSource {
    shared_fragments: SharedLayoutFragmentTree,
}

impl LayoutFragmentSource {
    pub(crate) fn new(webview_id: WebViewId) -> Self {
        Self {
            shared_fragments: shared_layout_fragment_tree_for(webview_id),
        }
    }

    pub(crate) fn fragments_arc(&self) -> Option<LayoutFragmentTree> {
        self.shared_fragments.get()
    }
}
