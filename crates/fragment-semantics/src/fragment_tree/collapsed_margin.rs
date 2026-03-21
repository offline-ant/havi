use app_units::Au;
use style::Zero;

use havi_types::geom::LogicalSides;

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

    pub fn solve(&self) -> Au {
        self.max_positive + self.min_negative
    }
}
