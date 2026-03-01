//! Shared fragment tree and geometry types for havi's layout and render crates.
//!
//! No dependency on DOM, style resolution, or layout engines — only stylo
//! computed values, app_units, euclid, and bitflags.

pub mod geom;
pub mod fragment_tree;

pub use fragment_tree::*;
pub use geom::{
    PhysicalPoint, PhysicalRect, PhysicalSides, PhysicalSize, PhysicalVec,
    LogicalVec2, LogicalRect, LogicalSides, LogicalSides1D,
    AuOrAuto, LengthPercentageOrAuto,
    ToLogical,
};
