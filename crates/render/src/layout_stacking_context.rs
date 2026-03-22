//! Minimal semantic paint sections for the rewritten render scene.

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum StackingContextSection {
    OwnBackgroundsAndBorders,
    DescendantBackgroundsAndBorders,
    Foreground,
    Outline,
}
