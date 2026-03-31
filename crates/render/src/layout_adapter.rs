use std::sync::Arc;

use libhavi::base::id::WebViewId;
use havi_types::FragmentArenaGeneration;
use libhavi::layout::{shared_layout_fragment_tree_for, SharedLayoutFragmentTree};

pub(crate) type LayoutFragmentTree = Arc<FragmentArenaGeneration>;

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
        self.shared_fragments.get::<FragmentArenaGeneration>()
    }
}
