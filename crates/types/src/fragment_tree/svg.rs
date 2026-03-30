use std::ops::Range;
use std::sync::Arc;

use app_units::Au;
use euclid::Transform2D;
use style_traits::CSSPixel;

use super::{BaseFragment, FragmentId, PaintChild, ShapedGlyph, Tag};
use crate::geom::{PhysicalPoint, PhysicalRect};

pub type SVGScalar = f32;
pub type SVGPoint = PhysicalPoint<SVGScalar>;
pub type SVGRect = PhysicalRect<SVGScalar>;
pub type SVGTransform = Transform2D<SVGScalar, CSSPixel, CSSPixel>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SVGColor {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SVGResourceId(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGCoordinateUnits {
    UserSpaceOnUse,
    ObjectBoundingBox,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGFillRule {
    NonZero,
    EvenOdd,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGLineCap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGLineJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGGradientSpreadMethod {
    Pad,
    Reflect,
    Repeat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGPreserveAspectRatioAlign {
    None,
    XMinYMin,
    XMidYMin,
    XMaxYMin,
    XMinYMid,
    XMidYMid,
    XMaxYMid,
    XMinYMax,
    XMidYMax,
    XMaxYMax,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGMeetOrSlice {
    Meet,
    Slice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SVGPreserveAspectRatio {
    pub align: SVGPreserveAspectRatioAlign,
    pub meet_or_slice: SVGMeetOrSlice,
}

impl Default for SVGPreserveAspectRatio {
    fn default() -> Self {
        Self {
            align: SVGPreserveAspectRatioAlign::XMidYMid,
            meet_or_slice: SVGMeetOrSlice::Meet,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGLengthUnit {
    Number,
    Px,
    Percent,
    In,
    Cm,
    Mm,
    Pt,
    Pc,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SVGLength {
    pub value: f32,
    pub unit: SVGLengthUnit,
}

impl SVGLength {
    pub fn zero() -> Self {
        Self {
            value: 0.0,
            unit: SVGLengthUnit::Number,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SVGPatternRect {
    pub x: SVGLength,
    pub y: SVGLength,
    pub width: SVGLength,
    pub height: SVGLength,
}

impl Default for SVGPatternRect {
    fn default() -> Self {
        Self {
            x: SVGLength::zero(),
            y: SVGLength::zero(),
            width: SVGLength::zero(),
            height: SVGLength::zero(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGTextAnchor {
    Start,
    Middle,
    End,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGPaintOrder {
    Normal,
    FillStrokeMarkers,
    FillMarkersStroke,
    StrokeFillMarkers,
    StrokeMarkersFill,
    MarkersFillStroke,
    MarkersStrokeFill,
}

impl Default for SVGPaintOrder {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGVectorEffect {
    None,
    NonScalingStroke,
}

impl Default for SVGVectorEffect {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug)]
pub enum SVGPaint {
    None,
    SolidColor(SVGColor),
    CurrentColor,
    ContextFill,
    ContextStroke,
    Server(SVGResourceId),
}

#[derive(Clone, Debug)]
pub struct SVGStrokeStyle {
    pub paint: SVGPaint,
    pub width: f32,
    pub opacity: f32,
    pub line_cap: SVGLineCap,
    pub line_join: SVGLineJoin,
    pub miter_limit: f32,
    pub dash_array: Vec<f32>,
    pub dash_offset: f32,
    pub vector_effect: SVGVectorEffect,
}

#[derive(Clone, Debug)]
pub struct SVGPaintStyle {
    pub fill: SVGPaint,
    pub fill_opacity: f32,
    pub stroke: Option<SVGStrokeStyle>,
    pub opacity: f32,
    pub paint_order: SVGPaintOrder,
}

impl Default for SVGPaintStyle {
    fn default() -> Self {
        Self {
            fill: SVGPaint::SolidColor(SVGColor {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0,
            }),
            fill_opacity: 1.0,
            stroke: None,
            opacity: 1.0,
            paint_order: SVGPaintOrder::Normal,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SVGEffectState {
    pub clip_path: Option<SVGResourceId>,
    pub mask: Option<SVGResourceId>,
    pub filter: Option<SVGResourceId>,
    pub marker_start: Option<SVGResourceId>,
    pub marker_mid: Option<SVGResourceId>,
    pub marker_end: Option<SVGResourceId>,
}

#[derive(Clone, Debug)]
pub struct SVGOverflowClip {
    pub enabled: bool,
    pub rect: SVGRect,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SVGUseInstanceChain {
    pub owner_tag: Tag,
    pub parent: Option<Box<SVGUseInstanceChain>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SVGFragmentIdentity {
    pub source_tag: Tag,
    pub instance_chain: Option<Box<SVGUseInstanceChain>>,
}

impl SVGFragmentIdentity {
    pub fn current_instance_owner_tag(&self) -> Option<Tag> {
        self.instance_chain.as_ref().map(|chain| chain.owner_tag)
    }

    pub fn current_instance_owner_or_source_tag(&self) -> Tag {
        self.current_instance_owner_tag().unwrap_or(self.source_tag)
    }
}

#[derive(Clone, Debug, Default)]
pub struct SVGBounds {
    pub object_bounding_box: SVGRect,
    pub stroke_bounding_box: SVGRect,
    pub decorated_bounding_box: SVGRect,
    pub visual_bounding_box: SVGRect,
}

#[derive(Clone, Debug)]
pub struct SVGViewportFragment {
    pub base: BaseFragment,
    pub identity: SVGFragmentIdentity,
    pub geometry_children: Vec<FragmentId>,
    pub paint_children: Vec<PaintChild>,
    pub viewport_rect: SVGRect,
    pub view_box_rect: Option<SVGRect>,
    pub local_to_parent_transform: SVGTransform,
    pub overflow_clip: Option<SVGOverflowClip>,
}

#[derive(Clone, Debug)]
pub enum SVGContainerKind {
    Group,
    ForeignObject {
        svg_viewport_rect: SVGRect,
    },
}

#[derive(Clone, Debug)]
pub struct SVGContainerFragment {
    pub base: BaseFragment,
    pub identity: SVGFragmentIdentity,
    pub kind: SVGContainerKind,
    pub geometry_children: Vec<FragmentId>,
    pub paint_children: Vec<PaintChild>,
    pub local_transform: SVGTransform,
    pub effects: SVGEffectState,
}

#[derive(Clone, Debug)]
pub struct SVGPathPayload {
    pub path: SVGPathData,
}

#[derive(Clone, Debug)]
pub struct SVGGlyphRun {
    pub text: String,
    pub rect: PhysicalRect<Au>,
    pub font_size_px: f32,
    pub glyphs: Vec<ShapedGlyph>,
    pub font_data: Option<Arc<Vec<u8>>>,
    pub font_index: u32,
    pub baseline_ascent: Au,
}

#[derive(Clone, Debug)]
pub struct SVGTextChunk {
    pub run_range: Range<u32>,
    pub anchor: SVGTextAnchor,
}

#[derive(Clone, Debug)]
pub struct SVGAddressableChar {
    pub run_index: u32,
    pub utf8_range: Range<u32>,
    pub position: SVGPoint,
    pub rotation: f32,
    pub hidden: bool,
    pub middle_of_cluster: bool,
    pub anchored_chunk_start: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SVGTextPayload {
    pub runs: Vec<SVGGlyphRun>,
    pub chunks: Vec<SVGTextChunk>,
    pub addressing: Vec<SVGAddressableChar>,
}

#[derive(Clone, Debug)]
pub struct SVGImagePayload {
    pub viewport_rect: SVGRect,
    pub href: Option<String>,
}

#[derive(Clone, Debug)]
pub enum SVGLeafKind {
    Path(SVGPathPayload),
    Text(SVGTextPayload),
    Image(SVGImagePayload),
}

#[derive(Clone, Debug)]
pub struct SVGLeafFragment {
    pub base: BaseFragment,
    pub identity: SVGFragmentIdentity,
    pub kind: SVGLeafKind,
    pub bounds: SVGBounds,
    pub local_transform: SVGTransform,
    pub paint: SVGPaintStyle,
    pub effects: SVGEffectState,
}

#[derive(Clone, Debug)]
pub enum SVGPathCommand {
    MoveTo(SVGPoint),
    LineTo(SVGPoint),
    QuadTo {
        ctrl: SVGPoint,
        to: SVGPoint,
    },
    CubicTo {
        ctrl1: SVGPoint,
        ctrl2: SVGPoint,
        to: SVGPoint,
    },
    Close,
}

#[derive(Clone, Debug)]
pub struct SVGPathData {
    pub fill_rule: SVGFillRule,
    pub commands: Vec<SVGPathCommand>,
}

#[derive(Clone, Debug)]
pub struct SVGResourceNode {
    pub kind: SVGResourceKind,
}

#[derive(Clone, Debug)]
pub enum SVGResourceKind {
    PaintServer(SVGPaintServerResource),
    ClipPath(SVGClipPathResource),
    Mask(SVGMaskResource),
    Filter(SVGFilterResource),
    Marker(SVGMarkerResource),
    UseInstanceSource(SVGUseInstanceSource),
}

#[derive(Clone, Debug)]
pub enum SVGPaintServerResource {
    Gradient(SVGGradientResource),
    Pattern(SVGPatternResource),
}

#[derive(Clone, Debug)]
pub struct SVGLinearGradient {
    pub start: SVGPoint,
    pub end: SVGPoint,
}

#[derive(Clone, Debug)]
pub struct SVGRadialGradient {
    pub center: SVGPoint,
    pub focal: SVGPoint,
    pub radius: f32,
    pub focal_radius: f32,
}

#[derive(Clone, Debug)]
pub enum SVGGradientKind {
    Linear(SVGLinearGradient),
    Radial(SVGRadialGradient),
}

#[derive(Clone, Debug)]
pub struct SVGGradientStop {
    pub offset: f32,
    pub color: SVGColor,
    pub opacity: f32,
}

#[derive(Clone, Debug)]
pub struct SVGGradientResource {
    pub units: SVGCoordinateUnits,
    pub gradient_transform: SVGTransform,
    pub spread_method: SVGGradientSpreadMethod,
    pub kind: SVGGradientKind,
    pub stops: Vec<SVGGradientStop>,
}

#[derive(Clone, Debug)]
pub struct SVGClipPathResource {
    pub units: SVGCoordinateUnits,
    pub transform: SVGTransform,
    pub paths: Vec<SVGPathData>,
}

#[derive(Clone, Debug)]
pub struct SVGMaskResource {
    pub units: SVGCoordinateUnits,
    pub content_units: SVGCoordinateUnits,
    pub rect: SVGRect,
}

#[derive(Clone, Debug)]
pub struct SVGFilterResource {
    pub rect: SVGRect,
}

#[derive(Clone, Debug)]
pub struct SVGMarkerResource {
    pub view_box: Option<SVGRect>,
    pub marker_units: SVGCoordinateUnits,
    pub orient_auto: bool,
}

#[derive(Clone, Debug)]
pub struct SVGPatternResource {
    pub units: SVGCoordinateUnits,
    pub content_units: SVGCoordinateUnits,
    pub pattern_transform: SVGTransform,
    pub rect: SVGPatternRect,
    pub view_box: Option<SVGRect>,
    pub preserve_aspect_ratio: SVGPreserveAspectRatio,
    pub source_fragment_roots: Vec<FragmentId>,
    pub source_resource_dependencies: Vec<SVGResourceId>,
}

#[derive(Clone, Debug)]
pub struct SVGUseInstanceSource {
    pub source_fragment_roots: Vec<FragmentId>,
    pub source_resource_dependencies: Vec<SVGResourceId>,
}
