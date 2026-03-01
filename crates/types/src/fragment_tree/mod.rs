// Fragment tree types produced by layout, adapted from Servo's fragment_tree/.
//
// A hierarchical tree of fragments: BoxFragment contains children,
// stores padding/border/margin/baselines.

mod base;
mod box_fragment;
mod fragment;
mod collapsed_margin;

pub use base::*;
pub use box_fragment::*;
pub use fragment::*;
pub use collapsed_margin::*;
