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
