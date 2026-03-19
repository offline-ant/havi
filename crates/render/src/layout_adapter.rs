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

use havi_types::Fragment;

/// The current shared fragment source used by havi-render.
///
/// Today this is still the converted `havi_types::Fragment` tree because leaf
/// extraction and embedder sharing are wired around it. Render code should go
/// through this adapter instead of treating the converted tree as the
/// architectural source of truth.
#[derive(Clone)]
pub(crate) struct LayoutFragmentSource {
    fragments: Arc<Vec<Fragment>>,
}

impl LayoutFragmentSource {
    pub(crate) fn new(fragments: Arc<Vec<Fragment>>) -> Self {
        Self { fragments }
    }

    pub(crate) fn fragments(&self) -> &[Fragment] {
        self.fragments.as_slice()
    }

}
