// Collapsed margins for block-level layout, adapted from Servo's fragment.rs.

use app_units::Au;
use style::Zero;

use crate::geom::LogicalSides;

/// Tracks margin collapse state for a block-level box.
#[derive(Clone, Debug)]
pub struct CollapsedBlockMargins {
    pub collapsed_through: bool,
    pub start: CollapsedMargin,
    pub end: CollapsedMargin,
}

impl CollapsedBlockMargins {
    pub fn from_margin(margin: &LogicalSides<Au>) -> Self {
        Self {
            collapsed_through: false,
            start: CollapsedMargin::new(margin.block_start),
            end: CollapsedMargin::new(margin.block_end),
        }
    }

    pub fn zero() -> Self {
        Self {
            collapsed_through: false,
            start: CollapsedMargin::zero(),
            end: CollapsedMargin::zero(),
        }
    }
}

/// A margin that has been collapsed with zero or more other margins.
/// Tracks the max positive and min negative components separately,
/// as per CSS2 §8.3.1.
#[derive(Clone, Copy, Debug)]
pub struct CollapsedMargin {
    max_positive: Au,
    min_negative: Au,
}

impl CollapsedMargin {
    pub fn zero() -> Self {
        Self {
            max_positive: Au::zero(),
            min_negative: Au::zero(),
        }
    }

    pub fn new(margin: Au) -> Self {
        Self {
            max_positive: margin.max(Au::zero()),
            min_negative: margin.min(Au::zero()),
        }
    }

    pub fn adjoin(&self, other: &Self) -> Self {
        Self {
            max_positive: self.max_positive.max(other.max_positive),
            min_negative: self.min_negative.min(other.min_negative),
        }
    }

    pub fn adjoin_assign(&mut self, other: &Self) {
        *self = self.adjoin(other);
    }

    /// The resulting margin value: max_positive + min_negative.
    pub fn solve(&self) -> Au {
        self.max_positive + self.min_negative
    }
}
