use euclid::Transform2D;
use style_traits::CSSPixel;

use super::{BaseFragment, FragmentId, PaintChild, Tag};
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

#[derive(Clone, Debug)]
pub enum SVGPaint {
    None,
    SolidColor(SVGColor),
    CurrentColor,
    Resource(SVGResourceId),
}

#[derive(Clone, Debug, Default)]
pub struct SVGResourceReferences {
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
pub struct SVGGroupFragment {
    pub base: BaseFragment,
    pub identity: SVGFragmentIdentity,
    pub geometry_children: Vec<FragmentId>,
    pub paint_children: Vec<PaintChild>,
    pub local_transform: SVGTransform,
    pub opacity: f32,
    pub resources: SVGResourceReferences,
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
pub struct SVGStrokeStyle {
    pub paint: SVGPaint,
    pub width: f32,
    pub opacity: f32,
    pub line_cap: SVGLineCap,
    pub line_join: SVGLineJoin,
    pub miter_limit: f32,
    pub non_scaling: bool,
}

#[derive(Clone, Debug)]
pub struct SVGPathFragment {
    pub base: BaseFragment,
    pub identity: SVGFragmentIdentity,
    pub path: SVGPathData,
    pub object_bounding_box: SVGRect,
    pub decorated_bounding_box: SVGRect,
    pub local_transform: SVGTransform,
    pub fill: SVGPaint,
    pub stroke: Option<SVGStrokeStyle>,
    pub resources: SVGResourceReferences,
}

#[derive(Clone, Debug)]
pub struct SVGGlyphRun {
    pub text: String,
    pub origin: SVGPoint,
    pub advance: f32,
    pub transform: SVGTransform,
}

#[derive(Clone, Debug)]
pub struct SVGTextFragment {
    pub base: BaseFragment,
    pub identity: SVGFragmentIdentity,
    pub glyph_runs: Vec<SVGGlyphRun>,
    pub object_bounding_box: SVGRect,
    pub decorated_bounding_box: SVGRect,
    pub local_transform: SVGTransform,
    pub resources: SVGResourceReferences,
}

#[derive(Clone, Debug)]
pub struct SVGForeignObjectFragment {
    pub base: BaseFragment,
    pub identity: SVGFragmentIdentity,
    pub geometry_children: Vec<FragmentId>,
    pub paint_children: Vec<PaintChild>,
    pub svg_viewport_rect: SVGRect,
    pub local_transform: SVGTransform,
}

#[derive(Clone, Debug)]
pub struct SVGImageFragment {
    pub base: BaseFragment,
    pub identity: SVGFragmentIdentity,
    pub viewport_rect: SVGRect,
    pub local_transform: SVGTransform,
    pub href: Option<String>,
    pub resources: SVGResourceReferences,
}

#[derive(Clone, Debug)]
pub struct SVGResourceNode {
    pub kind: SVGResourceKind,
}

#[derive(Clone, Debug)]
pub enum SVGResourceKind {
    Gradient(SVGGradientResource),
    ClipPath(SVGClipPathResource),
    Mask(SVGMaskResource),
    Filter(SVGFilterResource),
    Marker(SVGMarkerResource),
    Pattern(SVGPatternResource),
    UseInstanceSource(SVGUseInstanceSource),
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
    pub rect: SVGRect,
}

#[derive(Clone, Debug)]
pub struct SVGUseInstanceSource {
    pub source_fragment_roots: Vec<FragmentId>,
    pub source_resource_dependencies: Vec<SVGResourceId>,
}
