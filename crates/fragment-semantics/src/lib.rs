//! Shared render-facing semantic fragment tree types.
//!
//! This crate owns the active layout->render semantic boundary.
//! Layout lowers into this model. Render consumes this model directly.

pub mod fragment_tree;

pub use fragment_tree::*;
