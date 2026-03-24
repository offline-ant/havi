use std::sync::Arc;

use base::id::WebViewId;
use layout::fragment_tree::PublishedRootFragments;
use layout_api::{shared_layout_fragment_tree_for, SharedLayoutFragmentTree};

pub(crate) type LayoutFragmentTree = Arc<PublishedRootFragments>;

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
        self.shared_fragments.get::<PublishedRootFragments>()
    }
}
