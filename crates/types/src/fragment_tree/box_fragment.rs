use std::ops::Range;
use std::sync::Arc;

use app_units::Au;
use servo_arc::Arc as ServoArc;
use style::properties::ComputedValues;

use super::{
    BaseFragment, CollapsedBlockMargins, FragmentId, FragmentImageKey, ImageSourceKind,
    PaintChild,
};
use crate::geom::PhysicalSides;

#[derive(Clone, Copy, Debug, Default)]
pub struct Baselines {
    pub first: Option<Au>,
    pub last: Option<Au>,
}

#[derive(Clone, Debug)]
pub struct BackgroundImage {
    pub image_key: Option<FragmentImageKey>,
    pub source_kind: ImageSourceKind,
    pub svg_document_id: Option<u64>,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub data: Arc<Vec<u8>>,
    pub byte_range: Range<usize>,
}

#[derive(Clone, Debug)]
pub struct GridLayoutInfo {
    pub rows: Box<[Au]>,
    pub columns: Box<[Au]>,
}

#[derive(Clone, Debug)]
pub enum SpecificLayoutInfo {
    Grid(Box<GridLayoutInfo>),
    TableWrapper,
}

#[derive(Clone, Debug)]
pub struct BlockLevelLayoutInfo {
    pub clearance: Option<Au>,
    pub block_margins_collapsed_with_children: CollapsedBlockMargins,
}

#[derive(Clone, Debug)]
pub struct BoxFragment {
    pub base: BaseFragment,
    pub geometry_children: Vec<FragmentId>,
    pub paint_children: Vec<PaintChild>,
    pub padding: PhysicalSides<Au>,
    pub border: PhysicalSides<Au>,
    pub margin: PhysicalSides<Au>,
    pub baselines: Baselines,
    pub specific_layout_info: Option<SpecificLayoutInfo>,
    pub block_level_info: Option<Box<BlockLevelLayoutInfo>>,
}

impl BoxFragment {
    pub fn style(&self) -> &ServoArc<ComputedValues> {
        &self.base.style
    }

    pub fn content_rect(&self) -> crate::geom::PhysicalRect<Au> {
        self.base.rect
    }

    pub fn padding_rect(&self) -> crate::geom::PhysicalRect<Au> {
        self.content_rect().outer_rect(self.padding)
    }

    pub fn border_rect(&self) -> crate::geom::PhysicalRect<Au> {
        self.padding_rect().outer_rect(self.border)
    }

    pub fn margin_rect(&self) -> crate::geom::PhysicalRect<Au> {
        self.border_rect().outer_rect(self.margin)
    }

    pub fn padding_border_margin(&self) -> PhysicalSides<Au> {
        self.padding + self.border + self.margin
    }
}
