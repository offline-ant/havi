// Logical/physical geometry types for layout, adapted from Servo's layout/geom.rs.
//
// Layout uses logical coordinates (inline/block) internally and converts to
// physical coordinates (x/y) only at the rendering boundary.

use std::fmt;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

use app_units::Au;
use style::Zero;
use style::logical_geometry::{BlockFlowDirection, InlineBaseDirection, WritingMode};
use style::values::computed::{CSSPixelLength, LengthPercentage};
use style::values::generics::length::GenericLengthPercentageOrAuto as AutoOr;
use style_traits::CSSPixel;

pub type PhysicalPoint<U> = euclid::Point2D<U, CSSPixel>;
pub type PhysicalSize<U> = euclid::Size2D<U, CSSPixel>;
pub type PhysicalVec<U> = euclid::Vector2D<U, CSSPixel>;
pub type PhysicalRect<U> = euclid::Rect<U, CSSPixel>;
pub type PhysicalSides<U> = euclid::SideOffsets2D<U, CSSPixel>;
pub type AuOrAuto = AutoOr<Au>;
pub type LengthPercentageOrAuto<'a> = AutoOr<&'a LengthPercentage>;

// ---------------------------------------------------------------------------
// LogicalVec2
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub struct LogicalVec2<T> {
    pub inline: T,
    pub block: T,
}

impl<T: fmt::Debug> fmt::Debug for LogicalVec2<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Vec2 {{ i: {:?}, b: {:?} }}", self.inline, self.block)
    }
}

impl<T: Default> Default for LogicalVec2<T> {
    fn default() -> Self {
        Self {
            inline: T::default(),
            block: T::default(),
        }
    }
}

impl<T: Copy> From<T> for LogicalVec2<T> {
    fn from(value: T) -> Self {
        Self {
            inline: value,
            block: value,
        }
    }
}

impl<T> LogicalVec2<T> {
    pub fn map_inline_and_block_axes<U>(
        &self,
        inline_f: impl FnOnce(&T) -> U,
        block_f: impl FnOnce(&T) -> U,
    ) -> LogicalVec2<U> {
        LogicalVec2 {
            inline: inline_f(&self.inline),
            block: block_f(&self.block),
        }
    }
}

impl<T: Clone> LogicalVec2<T> {
    pub fn from_physical_size(physical_size: &PhysicalSize<T>, mode: WritingMode) -> Self {
        let (i, b) = if mode.is_horizontal() {
            (&physical_size.width, &physical_size.height)
        } else {
            (&physical_size.height, &physical_size.width)
        };
        LogicalVec2 {
            inline: i.clone(),
            block: b.clone(),
        }
    }

    pub fn map<U>(&self, f: impl Fn(&T) -> U) -> LogicalVec2<U> {
        LogicalVec2 {
            inline: f(&self.inline),
            block: f(&self.block),
        }
    }

    pub fn map_with<U, V>(
        &self,
        other: &LogicalVec2<U>,
        f: impl Fn(&T, &U) -> V,
    ) -> LogicalVec2<V> {
        LogicalVec2 {
            inline: f(&self.inline, &other.inline),
            block: f(&self.block, &other.block),
        }
    }

    pub fn to_physical_size(&self, mode: WritingMode) -> PhysicalSize<T> {
        let (x, y) = if mode.is_horizontal() {
            (&self.inline, &self.block)
        } else {
            (&self.block, &self.inline)
        };
        PhysicalSize::new(x.clone(), y.clone())
    }
}

impl<T: Copy + Neg<Output = T>> LogicalVec2<T> {
    pub fn to_physical_vector(&self, mode: WritingMode) -> PhysicalVec<T> {
        if mode.is_horizontal() {
            if mode.is_bidi_ltr() {
                PhysicalVec::new(self.inline, self.block)
            } else {
                PhysicalVec::new(-self.inline, self.block)
            }
        } else if mode.is_inline_tb() {
            PhysicalVec::new(self.block, self.inline)
        } else {
            PhysicalVec::new(-self.block, self.inline)
        }
    }
}

impl<T: Add<Output = T> + Copy> Add<LogicalVec2<T>> for LogicalVec2<T> {
    type Output = LogicalVec2<T>;
    fn add(self, other: Self) -> Self::Output {
        LogicalVec2 {
            inline: self.inline + other.inline,
            block: self.block + other.block,
        }
    }
}

impl<T: Sub<Output = T> + Copy> Sub<LogicalVec2<T>> for LogicalVec2<T> {
    type Output = LogicalVec2<T>;
    fn sub(self, other: Self) -> Self::Output {
        LogicalVec2 {
            inline: self.inline - other.inline,
            block: self.block - other.block,
        }
    }
}

impl<T: AddAssign<T> + Copy> AddAssign<LogicalVec2<T>> for LogicalVec2<T> {
    fn add_assign(&mut self, other: LogicalVec2<T>) {
        self.inline += other.inline;
        self.block += other.block;
    }
}

impl<T: SubAssign<T> + Copy> SubAssign<LogicalVec2<T>> for LogicalVec2<T> {
    fn sub_assign(&mut self, other: LogicalVec2<T>) {
        self.inline -= other.inline;
        self.block -= other.block;
    }
}

impl<T: Neg<Output = T> + Copy> Neg for LogicalVec2<T> {
    type Output = LogicalVec2<T>;
    fn neg(self) -> Self::Output {
        Self {
            inline: -self.inline,
            block: -self.block,
        }
    }
}

impl<T: Zero> LogicalVec2<T> {
    pub fn zero() -> Self {
        Self {
            inline: T::zero(),
            block: T::zero(),
        }
    }
}

impl<T: Clone> LogicalVec2<AutoOr<T>> {
    pub fn auto_is(&self, f: impl Fn() -> T) -> LogicalVec2<T> {
        self.map(|t| t.auto_is(&f))
    }
}

impl From<LogicalVec2<CSSPixelLength>> for LogicalVec2<Au> {
    fn from(value: LogicalVec2<CSSPixelLength>) -> Self {
        LogicalVec2 {
            inline: value.inline.into(),
            block: value.block.into(),
        }
    }
}

impl From<LogicalVec2<Au>> for LogicalVec2<CSSPixelLength> {
    fn from(value: LogicalVec2<Au>) -> Self {
        LogicalVec2 {
            inline: value.inline.into(),
            block: value.block.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// LogicalRect
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct LogicalRect<T> {
    pub start_corner: LogicalVec2<T>,
    pub size: LogicalVec2<T>,
}

impl<T: Zero> LogicalRect<T> {
    pub fn zero() -> Self {
        Self {
            start_corner: LogicalVec2::zero(),
            size: LogicalVec2::zero(),
        }
    }
}

impl fmt::Debug for LogicalRect<Au> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Rect(i{}×b{} @ (i{},b{}))",
            self.size.inline.to_f32_px(),
            self.size.block.to_f32_px(),
            self.start_corner.inline.to_f32_px(),
            self.start_corner.block.to_f32_px(),
        )
    }
}

impl<T> LogicalRect<T> {
    pub fn max_inline_position(&self) -> T
    where
        T: Add<Output = T> + Copy,
    {
        self.start_corner.inline + self.size.inline
    }

    pub fn max_block_position(&self) -> T
    where
        T: Add<Output = T> + Copy,
    {
        self.start_corner.block + self.size.block
    }

    pub fn inflate(&self, sides: &LogicalSides<T>) -> Self
    where
        T: Add<Output = T> + Sub<Output = T> + Copy,
    {
        Self {
            start_corner: LogicalVec2 {
                inline: self.start_corner.inline - sides.inline_start,
                block: self.start_corner.block - sides.block_start,
            },
            size: LogicalVec2 {
                inline: self.size.inline + sides.inline_sum(),
                block: self.size.block + sides.block_sum(),
            },
        }
    }

    pub fn deflate(&self, sides: &LogicalSides<T>) -> Self
    where
        T: Add<Output = T> + Sub<Output = T> + Copy,
    {
        LogicalRect {
            start_corner: LogicalVec2 {
                inline: self.start_corner.inline + sides.inline_start,
                block: self.start_corner.block + sides.block_start,
            },
            size: LogicalVec2 {
                inline: self.size.inline - sides.inline_sum(),
                block: self.size.block - sides.block_sum(),
            },
        }
    }
}

impl LogicalRect<Au> {
    /// Convert to physical coordinates. For horizontal-tb LTR this is a direct
    /// mapping of inline->x, block->y. The containing_block_inline_size is needed
    /// for RTL flipping.
    pub fn to_physical(&self, mode: WritingMode, containing_block_inline_size: Au) -> PhysicalRect<Au> {
        let (x, y, width, height) = if mode.is_vertical() {
            (
                self.start_corner.block,
                self.start_corner.inline,
                self.size.block,
                self.size.inline,
            )
        } else {
            let y = self.start_corner.block;
            let x = if mode.is_bidi_ltr() {
                self.start_corner.inline
            } else {
                containing_block_inline_size - self.max_inline_position()
            };
            (x, y, self.size.inline, self.size.block)
        };
        PhysicalRect::new(PhysicalPoint::new(x, y), PhysicalSize::new(width, height))
    }
}

impl From<LogicalRect<Au>> for LogicalRect<CSSPixelLength> {
    fn from(value: LogicalRect<Au>) -> Self {
        LogicalRect {
            start_corner: value.start_corner.into(),
            size: value.size.into(),
        }
    }
}

impl From<LogicalRect<CSSPixelLength>> for LogicalRect<Au> {
    fn from(value: LogicalRect<CSSPixelLength>) -> Self {
        LogicalRect {
            start_corner: value.start_corner.into(),
            size: value.size.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// LogicalSides
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
pub struct LogicalSides<T> {
    pub inline_start: T,
    pub inline_end: T,
    pub block_start: T,
    pub block_end: T,
}

impl<T: Clone> LogicalSides<T> {
    pub fn from_physical(sides: &PhysicalSides<T>, mode: WritingMode) -> Self {
        let block_flow = mode.block_flow_direction();
        let (bs, be) = match block_flow {
            BlockFlowDirection::TopToBottom => (&sides.top, &sides.bottom),
            BlockFlowDirection::RightToLeft => (&sides.right, &sides.left),
            BlockFlowDirection::LeftToRight => (&sides.left, &sides.right),
        };
        use BlockFlowDirection::TopToBottom;
        let (is, ie) = match (block_flow, mode.inline_base_direction()) {
            (TopToBottom, InlineBaseDirection::LeftToRight) => (&sides.left, &sides.right),
            (TopToBottom, InlineBaseDirection::RightToLeft) => (&sides.right, &sides.left),
            (_, InlineBaseDirection::LeftToRight) => (&sides.top, &sides.bottom),
            (_, InlineBaseDirection::RightToLeft) => (&sides.bottom, &sides.top),
        };
        LogicalSides {
            inline_start: is.clone(),
            inline_end: ie.clone(),
            block_start: bs.clone(),
            block_end: be.clone(),
        }
    }
}

impl<T> LogicalSides<T> {
    pub fn map<U>(&self, f: impl Fn(&T) -> U) -> LogicalSides<U> {
        LogicalSides {
            inline_start: f(&self.inline_start),
            inline_end: f(&self.inline_end),
            block_start: f(&self.block_start),
            block_end: f(&self.block_end),
        }
    }

    pub fn map_inline_and_block_axes<U>(
        &self,
        inline_f: impl Fn(&T) -> U,
        block_f: impl Fn(&T) -> U,
    ) -> LogicalSides<U> {
        LogicalSides {
            inline_start: inline_f(&self.inline_start),
            inline_end: inline_f(&self.inline_end),
            block_start: block_f(&self.block_start),
            block_end: block_f(&self.block_end),
        }
    }

    pub fn inline_sum(&self) -> T::Output
    where
        T: Add + Copy,
    {
        self.inline_start + self.inline_end
    }

    pub fn block_sum(&self) -> T::Output
    where
        T: Add + Copy,
    {
        self.block_start + self.block_end
    }

    pub fn sum(&self) -> LogicalVec2<T::Output>
    where
        T: Add + Copy,
    {
        LogicalVec2 {
            inline: self.inline_sum(),
            block: self.block_sum(),
        }
    }

    pub fn to_physical(&self, mode: WritingMode) -> PhysicalSides<T>
    where
        T: Clone,
    {
        let top;
        let right;
        let bottom;
        let left;
        if mode.is_vertical() {
            if mode.is_vertical_lr() {
                left = self.block_start.clone();
                right = self.block_end.clone();
            } else {
                right = self.block_start.clone();
                left = self.block_end.clone();
            }
            if mode.is_inline_tb() {
                top = self.inline_start.clone();
                bottom = self.inline_end.clone();
            } else {
                bottom = self.inline_start.clone();
                top = self.inline_end.clone();
            }
        } else {
            top = self.block_start.clone();
            bottom = self.block_end.clone();
            if mode.is_bidi_ltr() {
                left = self.inline_start.clone();
                right = self.inline_end.clone();
            } else {
                right = self.inline_start.clone();
                left = self.inline_end.clone();
            }
        }
        PhysicalSides::new(top, right, bottom, left)
    }
}

impl<T: Copy> LogicalSides<T> {
    pub fn start_offset(&self) -> LogicalVec2<T> {
        LogicalVec2 {
            inline: self.inline_start,
            block: self.block_start,
        }
    }

    #[inline]
    pub fn inline_sides(&self) -> LogicalSides1D<T> {
        LogicalSides1D::new(self.inline_start, self.inline_end)
    }

    #[inline]
    pub fn block_sides(&self) -> LogicalSides1D<T> {
        LogicalSides1D::new(self.block_start, self.block_end)
    }
}

impl LogicalSides<LengthPercentage> {
    pub fn percentages_relative_to(&self, basis: Au) -> LogicalSides<Au> {
        self.map(|value| value.to_used_value(basis))
    }
}

impl LogicalSides<LengthPercentageOrAuto<'_>> {
    pub fn percentages_relative_to(&self, basis: Au) -> LogicalSides<AuOrAuto> {
        self.map(|value| value.map(|value| value.to_used_value(basis)))
    }
}

impl<T: Clone> LogicalSides<AutoOr<T>> {
    pub fn auto_is(&self, f: impl Fn() -> T) -> LogicalSides<T> {
        self.map(|s| s.auto_is(&f))
    }
}

impl<T: Add<Output = T> + Copy> Add<LogicalSides<T>> for LogicalSides<T> {
    type Output = LogicalSides<T>;
    fn add(self, other: Self) -> Self::Output {
        LogicalSides {
            inline_start: self.inline_start + other.inline_start,
            inline_end: self.inline_end + other.inline_end,
            block_start: self.block_start + other.block_start,
            block_end: self.block_end + other.block_end,
        }
    }
}

impl<T: Sub<Output = T> + Copy> Sub<LogicalSides<T>> for LogicalSides<T> {
    type Output = LogicalSides<T>;
    fn sub(self, other: Self) -> Self::Output {
        LogicalSides {
            inline_start: self.inline_start - other.inline_start,
            inline_end: self.inline_end - other.inline_end,
            block_start: self.block_start - other.block_start,
            block_end: self.block_end - other.block_end,
        }
    }
}

impl<T: Neg<Output = T> + Copy> Neg for LogicalSides<T> {
    type Output = LogicalSides<T>;
    fn neg(self) -> Self::Output {
        Self {
            inline_start: -self.inline_start,
            inline_end: -self.inline_end,
            block_start: -self.block_start,
            block_end: -self.block_end,
        }
    }
}

impl<T: Zero> LogicalSides<T> {
    pub fn zero() -> LogicalSides<T> {
        Self {
            inline_start: T::zero(),
            inline_end: T::zero(),
            block_start: T::zero(),
            block_end: T::zero(),
        }
    }
}

impl From<LogicalSides<CSSPixelLength>> for LogicalSides<Au> {
    fn from(value: LogicalSides<CSSPixelLength>) -> Self {
        Self {
            inline_start: value.inline_start.into(),
            inline_end: value.inline_end.into(),
            block_start: value.block_start.into(),
            block_end: value.block_end.into(),
        }
    }
}

impl From<LogicalSides<Au>> for LogicalSides<CSSPixelLength> {
    fn from(value: LogicalSides<Au>) -> Self {
        Self {
            inline_start: value.inline_start.into(),
            inline_end: value.inline_end.into(),
            block_start: value.block_start.into(),
            block_end: value.block_end.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// LogicalSides1D
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
pub struct LogicalSides1D<T> {
    pub start: T,
    pub end: T,
}

impl<T> LogicalSides1D<T> {
    #[inline]
    pub fn new(start: T, end: T) -> Self {
        Self { start, end }
    }

    pub fn map<U>(&self, f: impl Fn(&T) -> U) -> LogicalSides1D<U> {
        LogicalSides1D {
            start: f(&self.start),
            end: f(&self.end),
        }
    }
}

impl<T> LogicalSides1D<AutoOr<T>> {
    #[inline]
    pub fn either_specified(&self) -> bool {
        !self.start.is_auto() || !self.end.is_auto()
    }

    #[inline]
    pub fn either_auto(&self) -> bool {
        self.start.is_auto() || self.end.is_auto()
    }
}

impl LogicalSides1D<AutoOr<&LengthPercentage>> {
    pub fn percentages_relative_to(&self, basis: Au) -> LogicalSides1D<AutoOr<Au>> {
        self.map(|value| value.map(|length_percentage| length_percentage.to_used_value(basis)))
    }
}

// ---------------------------------------------------------------------------
// ToLogical helper traits
// ---------------------------------------------------------------------------

pub trait ToLogical<Unit, LogicalType> {
    fn to_logical(&self, writing_mode: WritingMode) -> LogicalType;
}

impl<Unit: Copy> ToLogical<Unit, LogicalVec2<Unit>> for PhysicalSize<Unit> {
    fn to_logical(&self, writing_mode: WritingMode) -> LogicalVec2<Unit> {
        LogicalVec2::from_physical_size(self, writing_mode)
    }
}

impl<Unit: Copy> ToLogical<Unit, LogicalSides<Unit>> for PhysicalSides<Unit> {
    fn to_logical(&self, writing_mode: WritingMode) -> LogicalSides<Unit> {
        LogicalSides::from_physical(self, writing_mode)
    }
}
