use std::ops::Range;
use std::sync::Arc;

use app_units::Au;
use base::id::PipelineId;
use style::logical_geometry::WritingMode;
use style::computed_values::position::T as Position;
use style::values::specified::align::AlignFlags;

use super::{
    BaseFragment, BoxFragment, SVGForeignObjectFragment, SVGGroupFragment, SVGImageFragment,
    SVGPathFragment, SVGTextFragment, SVGViewportFragment,
};
use crate::geom::{LogicalVec2, PhysicalRect, PhysicalSides};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FragmentId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlacementId(pub u32);

#[derive(Clone, Debug)]
pub enum PaintChild {
    Fragment(FragmentId),
    Placement(PlacementId),
}

#[derive(Clone, Debug)]
pub struct FragmentNode {
    pub parent: Option<FragmentId>,
    pub kind: FragmentKind,
}

impl FragmentNode {
    pub fn base(&self) -> &BaseFragment {
        match &self.kind {
            FragmentKind::Box(fragment) | FragmentKind::Float(fragment) => &fragment.base,
            FragmentKind::Positioning(fragment) => &fragment.base,
            FragmentKind::Text(fragment) => &fragment.base,
            FragmentKind::Image(fragment) => &fragment.base,
            FragmentKind::IFrame(fragment) => &fragment.base,
            FragmentKind::SVGViewport(fragment) => &fragment.base,
            FragmentKind::SVGGroup(fragment) => &fragment.base,
            FragmentKind::SVGPath(fragment) => &fragment.base,
            FragmentKind::SVGText(fragment) => &fragment.base,
            FragmentKind::SVGForeignObject(fragment) => &fragment.base,
            FragmentKind::SVGImage(fragment) => &fragment.base,
        }
    }
}

#[derive(Clone, Debug)]
pub enum FragmentKind {
    Box(BoxFragment),
    Float(BoxFragment),
    Positioning(PositioningFragment),
    Text(TextFragment),
    Image(ImageFragment),
    IFrame(IFrameFragment),
    SVGViewport(SVGViewportFragment),
    SVGGroup(SVGGroupFragment),
    SVGPath(SVGPathFragment),
    SVGText(SVGTextFragment),
    SVGForeignObject(SVGForeignObjectFragment),
    SVGImage(SVGImageFragment),
}

impl FragmentKind {
    pub fn base(&self) -> &BaseFragment {
        match self {
            FragmentKind::Box(fragment) | FragmentKind::Float(fragment) => &fragment.base,
            FragmentKind::Positioning(fragment) => &fragment.base,
            FragmentKind::Text(fragment) => &fragment.base,
            FragmentKind::Image(fragment) => &fragment.base,
            FragmentKind::IFrame(fragment) => &fragment.base,
            FragmentKind::SVGViewport(fragment) => &fragment.base,
            FragmentKind::SVGGroup(fragment) => &fragment.base,
            FragmentKind::SVGPath(fragment) => &fragment.base,
            FragmentKind::SVGText(fragment) => &fragment.base,
            FragmentKind::SVGForeignObject(fragment) => &fragment.base,
            FragmentKind::SVGImage(fragment) => &fragment.base,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ShapedGlyph {
    pub glyph_id: u32,
    pub advance: Au,
    pub x_offset: Au,
    pub y_offset: Au,
    pub char_count: u32,
}

#[derive(Clone, Debug)]
pub struct TextFragment {
    pub base: BaseFragment,
    pub text: String,
    pub font_size_px: f32,
    pub glyphs: Vec<ShapedGlyph>,
    pub font_data: Option<Arc<Vec<u8>>>,
    pub font_index: u32,
    pub baseline_ascent: Au,
    pub underline_offset: Au,
    pub underline_size: Au,
    pub strikeout_offset: Au,
    pub strikeout_size: Au,
    pub character_range_start: u32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FragmentImageKey {
    pub namespace: u32,
    pub image: u32,
}

impl FragmentImageKey {
    pub fn packed(self) -> u64 {
        ((self.namespace as u64) << 32) | self.image as u64
    }
}

impl From<(u32, u32)> for FragmentImageKey {
    fn from((namespace, image): (u32, u32)) -> Self {
        Self { namespace, image }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ImageSourceKind {
    Raster,
    SvgDocument,
    Canvas,
    Video,
}

#[derive(Clone, Debug)]
pub struct ImageFragment {
    pub base: BaseFragment,
    pub image_key: Option<FragmentImageKey>,
    pub source_kind: ImageSourceKind,
    pub svg_document_id: Option<u64>,
    pub image_revision: u64,
    pub frame_width: u32,
    pub frame_height: u32,
    pub image_data: Arc<Vec<u8>>,
    pub frame_byte_range: Range<usize>,
}

#[derive(Clone, Debug)]
pub struct SharedImageSource {
    pub data: Arc<Vec<u8>>,
    pub offset: usize,
    pub width: u32,
    pub height: u32,
    pub revision: u64,
}

pub type SharedImageSourceMap = std::collections::HashMap<FragmentImageKey, SharedImageSource>;

#[derive(Clone, Debug)]
pub struct IFrameFragment {
    pub base: BaseFragment,
    pub pipeline_id: PipelineId,
}

#[derive(Clone, Debug)]
pub struct PositioningFragment {
    pub base: BaseFragment,
    pub geometry_children: Vec<FragmentId>,
    pub paint_children: Vec<PaintChild>,
}

#[derive(Clone, Debug)]
pub struct OutOfFlowPlacement {
    pub fragment: FragmentId,
    pub containing_block: Option<FragmentId>,
    pub static_position_rect: PhysicalRect<Au>,
    pub resolved_alignment: LogicalVec2<AlignFlags>,
    pub original_parent_writing_mode: WritingMode,
    pub position: Position,
}

#[derive(Clone, Debug)]
pub struct FragmentDerivedData {
    pub containing_blocks: Vec<PhysicalRect<Au>>,
    pub scrollable_overflow: Vec<PhysicalRect<Au>>,
    pub sticky_insets: Vec<Option<PhysicalSides<crate::geom::AuOrAuto>>>,
    pub background_images: Vec<Vec<Option<super::BackgroundImage>>>,
}
