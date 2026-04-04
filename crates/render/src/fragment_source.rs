use libhavi::base::id::WebViewId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FragmentSourceIdentity {
    pub webview_id: WebViewId,
    pub generation: u64,
}

pub struct CachedFragmentSource {
    identity: FragmentSourceIdentity,
}

impl CachedFragmentSource {
    pub fn new(identity: FragmentSourceIdentity) -> Self {
        Self { identity }
    }

    pub fn identity(&self) -> FragmentSourceIdentity {
        self.identity
    }

    pub fn is_valid_for(&self, identity: FragmentSourceIdentity) -> bool {
        self.identity == identity
    }
}
