use std::sync::Arc;

use havi_types::Fragment;

/// Cached fragment source for the active render path.
///
/// This cache only owns the shared fragment `Arc` and provides change
/// detection. Paint ordering and frame construction are derived from semantic
/// layout traversal at render time.
pub struct CachedFragmentSource {
    fragments: Arc<Vec<Fragment>>,
    frag_ptr: usize,
}

impl CachedFragmentSource {
    pub fn new(fragments: Arc<Vec<Fragment>>) -> Self {
        let frag_ptr = Arc::as_ptr(&fragments) as usize;
        Self { fragments, frag_ptr }
    }

    pub fn is_valid_for(&self, fragments: &Arc<Vec<Fragment>>) -> bool {
        Arc::as_ptr(fragments) as usize == self.frag_ptr
    }

    pub(crate) fn fragments_arc(&self) -> &Arc<Vec<Fragment>> {
        &self.fragments
    }
}
