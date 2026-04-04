use std::sync::Arc;

use havi_types::FragmentArenaGeneration;
use layout_api::{shared_layout_fragment_tree_for, SharedLayoutFragmentTree};
use libhavi::base::id::WebViewId;

use crate::fragment_source::FragmentSourceIdentity;

pub(crate) type LayoutFragmentTree = Arc<FragmentArenaGeneration>;

pub(crate) struct LayoutFragmentSnapshot {
    pub(crate) identity: FragmentSourceIdentity,
    pub(crate) fragments: LayoutFragmentTree,
}

#[derive(Clone)]
pub(crate) struct LayoutFragmentSource {
    shared_fragments: SharedLayoutFragmentTree,
    webview_id: WebViewId,
}

impl LayoutFragmentSource {
    pub(crate) fn new(webview_id: WebViewId) -> Self {
        Self {
            shared_fragments: shared_layout_fragment_tree_for(webview_id),
            webview_id,
        }
    }

    pub(crate) fn snapshot(&self) -> Option<LayoutFragmentSnapshot> {
        let snapshot = self
            .shared_fragments
            .snapshot::<FragmentArenaGeneration>()?;
        Some(LayoutFragmentSnapshot {
            identity: FragmentSourceIdentity {
                webview_id: self.webview_id,
                generation: snapshot.generation,
            },
            fragments: snapshot.payload,
        })
    }
}
