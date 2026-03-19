/// Cached fragment source for the active render path.
///
/// This cache tracks the shared layout fragment tree identity. Paint ordering
/// and frame construction are derived from semantic layout traversal at render
/// time.
pub struct CachedFragmentSource {
    frag_ptr: usize,
}

impl CachedFragmentSource {
    pub fn new(frag_ptr: usize) -> Self {
        Self { frag_ptr }
    }

    pub fn is_valid_for(&self, frag_ptr: usize) -> bool {
        self.frag_ptr == frag_ptr
    }
}
