use rustc_hash::FxHashMap;
use servo_arc::Arc as ServoArc;
use style::dom::OpaqueNode;
use style::properties::ComputedValues;

use havi_types::fragment_tree::SVGCoordinateUnits;
use crate::layout::fragment_tree::Tag;
use layout_api::wrapper_traits::PseudoElementChain;
use layout_api::{
    SVGClipPathData, SVGCommonData, SVGElementData, SVGFilterData, SVGForeignObjectData,
    SVGGeometryData, SVGGradientData, SVGImageData, SVGMarkerData, SVGMaskData, SVGNodeKind,
    SVGPaintData, SVGPatternData, SVGReferenceValue, SVGStopData, SVGTextData, SVGTextPathData,
    SVGUseData, SVGViewportData,
};

use super::dom::{SVGLayoutNodeSummary, SVGNodeResolvedStyle, summarize_node_kind};
use super::style::{
    SVGGeometryStyle, SVGTextStyle, resolve_geometry_style, resolve_text_style,
    resolve_viewport_style,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SVGNodeId(pub usize);

#[derive(Clone, Copy, Debug, Default)]
pub struct SVGNodeMetadata {
    pub preserve_aspect_ratio_specified: bool,
}

#[derive(Clone, Debug)]
pub struct SVGOwnedReferenceValue {
    pub raw: String,
    pub local_reference: Option<String>,
}

impl SVGOwnedReferenceValue {
    fn borrowed(&self) -> SVGReferenceValue<'_> {
        SVGReferenceValue {
            raw: &self.raw,
            local_reference: self.local_reference.as_deref(),
        }
    }
}

impl From<SVGReferenceValue<'_>> for SVGOwnedReferenceValue {
    fn from(value: SVGReferenceValue<'_>) -> Self {
        Self {
            raw: value.raw.to_owned(),
            local_reference: value.local_reference.map(str::to_owned),
        }
    }
}

#[derive(Clone, Debug)]
pub enum SVGOwnedGeometryData {
    Path {
        d: Option<String>,
    },
    Rect {
        x: Option<layout_api::SVGLengthValue>,
        y: Option<layout_api::SVGLengthValue>,
        width: Option<layout_api::SVGLengthValue>,
        height: Option<layout_api::SVGLengthValue>,
        rx: Option<layout_api::SVGLengthValue>,
        ry: Option<layout_api::SVGLengthValue>,
    },
    Circle {
        cx: Option<layout_api::SVGLengthValue>,
        cy: Option<layout_api::SVGLengthValue>,
        r: Option<layout_api::SVGLengthValue>,
    },
    Ellipse {
        cx: Option<layout_api::SVGLengthValue>,
        cy: Option<layout_api::SVGLengthValue>,
        rx: Option<layout_api::SVGLengthValue>,
        ry: Option<layout_api::SVGLengthValue>,
    },
    Line {
        x1: Option<layout_api::SVGLengthValue>,
        y1: Option<layout_api::SVGLengthValue>,
        x2: Option<layout_api::SVGLengthValue>,
        y2: Option<layout_api::SVGLengthValue>,
    },
    Polyline {
        points: Option<String>,
    },
    Polygon {
        points: Option<String>,
    },
}

impl SVGOwnedGeometryData {
    fn borrowed(&self) -> SVGGeometryData<'_> {
        match self {
            Self::Path { d } => SVGGeometryData::Path { d: d.as_deref() },
            Self::Rect {
                x,
                y,
                width,
                height,
                rx,
                ry,
            } => SVGGeometryData::Rect {
                x: *x,
                y: *y,
                width: *width,
                height: *height,
                rx: *rx,
                ry: *ry,
            },
            Self::Circle { cx, cy, r } => SVGGeometryData::Circle {
                cx: *cx,
                cy: *cy,
                r: *r,
            },
            Self::Ellipse { cx, cy, rx, ry } => SVGGeometryData::Ellipse {
                cx: *cx,
                cy: *cy,
                rx: *rx,
                ry: *ry,
            },
            Self::Line { x1, y1, x2, y2 } => SVGGeometryData::Line {
                x1: *x1,
                y1: *y1,
                x2: *x2,
                y2: *y2,
            },
            Self::Polyline { points } => SVGGeometryData::Polyline {
                points: points.as_deref(),
            },
            Self::Polygon { points } => SVGGeometryData::Polygon {
                points: points.as_deref(),
            },
        }
    }
}

impl From<&SVGGeometryData<'_>> for SVGOwnedGeometryData {
    fn from(value: &SVGGeometryData<'_>) -> Self {
        match value {
            SVGGeometryData::Path { d } => Self::Path {
                d: d.map(str::to_owned),
            },
            SVGGeometryData::Rect {
                x,
                y,
                width,
                height,
                rx,
                ry,
            } => Self::Rect {
                x: *x,
                y: *y,
                width: *width,
                height: *height,
                rx: *rx,
                ry: *ry,
            },
            SVGGeometryData::Circle { cx, cy, r } => Self::Circle {
                cx: *cx,
                cy: *cy,
                r: *r,
            },
            SVGGeometryData::Ellipse { cx, cy, rx, ry } => Self::Ellipse {
                cx: *cx,
                cy: *cy,
                rx: *rx,
                ry: *ry,
            },
            SVGGeometryData::Line { x1, y1, x2, y2 } => Self::Line {
                x1: *x1,
                y1: *y1,
                x2: *x2,
                y2: *y2,
            },
            SVGGeometryData::Polyline { points } => Self::Polyline {
                points: points.map(str::to_owned),
            },
            SVGGeometryData::Polygon { points } => Self::Polygon {
                points: points.map(str::to_owned),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct SVGOwnedPaintData {
    pub color: Option<String>,
    pub fill: Option<String>,
    pub fill_opacity: Option<f32>,
    pub fill_rule: Option<havi_types::fragment_tree::SVGFillRule>,
    pub stroke: Option<String>,
    pub stroke_opacity: Option<f32>,
    pub stroke_width: Option<layout_api::SVGLengthValue>,
    pub stroke_linejoin: Option<havi_types::fragment_tree::SVGLineJoin>,
    pub stroke_linecap: Option<havi_types::fragment_tree::SVGLineCap>,
    pub stroke_miterlimit: Option<f32>,
    pub stroke_dasharray: Option<Vec<f32>>,
    pub stroke_dashoffset: Option<f32>,
    pub paint_order: Option<havi_types::fragment_tree::SVGPaintOrder>,
    pub opacity: Option<f32>,
    pub pointer_events: Option<layout_api::SVGPointerEventsValue>,
    pub vector_effect: Option<havi_types::fragment_tree::SVGVectorEffect>,
    pub clip_rule: Option<havi_types::fragment_tree::SVGFillRule>,
    pub clip_path: Option<SVGOwnedReferenceValue>,
    pub mask: Option<SVGOwnedReferenceValue>,
    pub filter: Option<SVGOwnedReferenceValue>,
    pub marker_start: Option<SVGOwnedReferenceValue>,
    pub marker_mid: Option<SVGOwnedReferenceValue>,
    pub marker_end: Option<SVGOwnedReferenceValue>,
}

impl SVGOwnedPaintData {
    fn borrowed(&self) -> SVGPaintData<'_> {
        SVGPaintData {
            color: self.color.as_deref(),
            fill: self.fill.as_deref(),
            fill_opacity: self.fill_opacity,
            fill_rule: self.fill_rule,
            stroke: self.stroke.as_deref(),
            stroke_opacity: self.stroke_opacity,
            stroke_width: self.stroke_width,
            stroke_linejoin: self.stroke_linejoin,
            stroke_linecap: self.stroke_linecap,
            stroke_miterlimit: self.stroke_miterlimit,
            stroke_dasharray: self.stroke_dasharray.clone(),
            stroke_dashoffset: self.stroke_dashoffset,
            paint_order: self.paint_order,
            opacity: self.opacity,
            pointer_events: self.pointer_events,
            vector_effect: self.vector_effect,
            clip_rule: self.clip_rule,
            clip_path: self.clip_path.as_ref().map(SVGOwnedReferenceValue::borrowed),
            mask: self.mask.as_ref().map(SVGOwnedReferenceValue::borrowed),
            filter: self.filter.as_ref().map(SVGOwnedReferenceValue::borrowed),
            marker_start: self
                .marker_start
                .as_ref()
                .map(SVGOwnedReferenceValue::borrowed),
            marker_mid: self.marker_mid.as_ref().map(SVGOwnedReferenceValue::borrowed),
            marker_end: self.marker_end.as_ref().map(SVGOwnedReferenceValue::borrowed),
        }
    }
}

impl From<&SVGPaintData<'_>> for SVGOwnedPaintData {
    fn from(value: &SVGPaintData<'_>) -> Self {
        Self {
            color: value.color.map(str::to_owned),
            fill: value.fill.map(str::to_owned),
            fill_opacity: value.fill_opacity,
            fill_rule: value.fill_rule,
            stroke: value.stroke.map(str::to_owned),
            stroke_opacity: value.stroke_opacity,
            stroke_width: value.stroke_width,
            stroke_linejoin: value.stroke_linejoin,
            stroke_linecap: value.stroke_linecap,
            stroke_miterlimit: value.stroke_miterlimit,
            stroke_dasharray: value.stroke_dasharray.clone(),
            stroke_dashoffset: value.stroke_dashoffset,
            paint_order: value.paint_order,
            opacity: value.opacity,
            pointer_events: value.pointer_events,
            vector_effect: value.vector_effect,
            clip_rule: value.clip_rule,
            clip_path: value.clip_path.map(Into::into),
            mask: value.mask.map(Into::into),
            filter: value.filter.map(Into::into),
            marker_start: value.marker_start.map(Into::into),
            marker_mid: value.marker_mid.map(Into::into),
            marker_end: value.marker_end.map(Into::into),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SVGOwnedUseData {
    pub href: Option<SVGOwnedReferenceValue>,
    pub x: Option<layout_api::SVGLengthValue>,
    pub y: Option<layout_api::SVGLengthValue>,
    pub width: Option<layout_api::SVGLengthValue>,
    pub height: Option<layout_api::SVGLengthValue>,
}

impl SVGOwnedUseData {
    fn borrowed(&self) -> SVGUseData<'_> {
        SVGUseData {
            href: self.href.as_ref().map(SVGOwnedReferenceValue::borrowed),
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

impl From<&SVGUseData<'_>> for SVGOwnedUseData {
    fn from(value: &SVGUseData<'_>) -> Self {
        Self {
            href: value.href.map(Into::into),
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
        }
    }
}

#[derive(Clone, Debug)]
pub enum SVGOwnedGradientData {
    Linear {
        href: Option<SVGOwnedReferenceValue>,
        x1: Option<layout_api::SVGLengthValue>,
        y1: Option<layout_api::SVGLengthValue>,
        x2: Option<layout_api::SVGLengthValue>,
        y2: Option<layout_api::SVGLengthValue>,
        gradient_units: Option<SVGCoordinateUnits>,
        gradient_transform: layout_api::SVGTransformListValue,
        spread_method: Option<havi_types::fragment_tree::SVGGradientSpreadMethod>,
    },
    Radial {
        href: Option<SVGOwnedReferenceValue>,
        cx: Option<layout_api::SVGLengthValue>,
        cy: Option<layout_api::SVGLengthValue>,
        r: Option<layout_api::SVGLengthValue>,
        fx: Option<layout_api::SVGLengthValue>,
        fy: Option<layout_api::SVGLengthValue>,
        fr: Option<layout_api::SVGLengthValue>,
        gradient_units: Option<SVGCoordinateUnits>,
        gradient_transform: layout_api::SVGTransformListValue,
        spread_method: Option<havi_types::fragment_tree::SVGGradientSpreadMethod>,
    },
}

impl SVGOwnedGradientData {
    fn borrowed(&self) -> SVGGradientData<'_> {
        match self {
            Self::Linear {
                href,
                x1,
                y1,
                x2,
                y2,
                gradient_units,
                gradient_transform,
                spread_method,
            } => SVGGradientData::Linear {
                href: href.as_ref().map(SVGOwnedReferenceValue::borrowed),
                x1: *x1,
                y1: *y1,
                x2: *x2,
                y2: *y2,
                gradient_units: *gradient_units,
                gradient_transform: gradient_transform.clone(),
                spread_method: *spread_method,
            },
            Self::Radial {
                href,
                cx,
                cy,
                r,
                fx,
                fy,
                fr,
                gradient_units,
                gradient_transform,
                spread_method,
            } => SVGGradientData::Radial {
                href: href.as_ref().map(SVGOwnedReferenceValue::borrowed),
                cx: *cx,
                cy: *cy,
                r: *r,
                fx: *fx,
                fy: *fy,
                fr: *fr,
                gradient_units: *gradient_units,
                gradient_transform: gradient_transform.clone(),
                spread_method: *spread_method,
            },
        }
    }
}

impl From<&SVGGradientData<'_>> for SVGOwnedGradientData {
    fn from(value: &SVGGradientData<'_>) -> Self {
        match value {
            SVGGradientData::Linear {
                href,
                x1,
                y1,
                x2,
                y2,
                gradient_units,
                gradient_transform,
                spread_method,
            } => Self::Linear {
                href: href.map(Into::into),
                x1: *x1,
                y1: *y1,
                x2: *x2,
                y2: *y2,
                gradient_units: *gradient_units,
                gradient_transform: gradient_transform.clone(),
                spread_method: *spread_method,
            },
            SVGGradientData::Radial {
                href,
                cx,
                cy,
                r,
                fx,
                fy,
                fr,
                gradient_units,
                gradient_transform,
                spread_method,
            } => Self::Radial {
                href: href.map(Into::into),
                cx: *cx,
                cy: *cy,
                r: *r,
                fx: *fx,
                fy: *fy,
                fr: *fr,
                gradient_units: *gradient_units,
                gradient_transform: gradient_transform.clone(),
                spread_method: *spread_method,
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct SVGOwnedStopData {
    pub offset: Option<f32>,
    pub stop_color: Option<String>,
    pub stop_opacity: Option<String>,
}

impl SVGOwnedStopData {
    fn borrowed(&self) -> SVGStopData<'_> {
        SVGStopData {
            offset: self.offset,
            stop_color: self.stop_color.as_deref(),
            stop_opacity: self.stop_opacity.as_deref(),
        }
    }
}

impl From<&SVGStopData<'_>> for SVGOwnedStopData {
    fn from(value: &SVGStopData<'_>) -> Self {
        Self {
            offset: value.offset,
            stop_color: value.stop_color.map(str::to_owned),
            stop_opacity: value.stop_opacity.map(str::to_owned),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SVGOwnedPatternData {
    pub href: Option<SVGOwnedReferenceValue>,
    pub x: Option<layout_api::SVGLengthValue>,
    pub y: Option<layout_api::SVGLengthValue>,
    pub width: Option<layout_api::SVGLengthValue>,
    pub height: Option<layout_api::SVGLengthValue>,
    pub pattern_units: Option<SVGCoordinateUnits>,
    pub pattern_content_units: Option<SVGCoordinateUnits>,
    pub pattern_transform: layout_api::SVGTransformListValue,
    pub view_box: Option<layout_api::SVGRectValue>,
    pub preserve_aspect_ratio: layout_api::SVGPreserveAspectRatioValue,
}

impl SVGOwnedPatternData {
    fn borrowed(&self) -> SVGPatternData<'_> {
        SVGPatternData {
            href: self.href.as_ref().map(SVGOwnedReferenceValue::borrowed),
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            pattern_units: self.pattern_units,
            pattern_content_units: self.pattern_content_units,
            pattern_transform: self.pattern_transform.clone(),
            view_box: self.view_box,
            preserve_aspect_ratio: self.preserve_aspect_ratio,
        }
    }
}

impl From<&SVGPatternData<'_>> for SVGOwnedPatternData {
    fn from(value: &SVGPatternData<'_>) -> Self {
        Self {
            href: value.href.map(Into::into),
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
            pattern_units: value.pattern_units,
            pattern_content_units: value.pattern_content_units,
            pattern_transform: value.pattern_transform.clone(),
            view_box: value.view_box,
            preserve_aspect_ratio: value.preserve_aspect_ratio,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SVGOwnedTextPathData {
    pub href: Option<SVGOwnedReferenceValue>,
    pub start_offset: Option<layout_api::SVGLengthValue>,
    pub text: SVGTextData,
}

impl SVGOwnedTextPathData {
    fn borrowed(&self) -> SVGTextPathData<'_> {
        SVGTextPathData {
            href: self.href.as_ref().map(SVGOwnedReferenceValue::borrowed),
            start_offset: self.start_offset,
            text: self.text.clone(),
        }
    }
}

impl From<&SVGTextPathData<'_>> for SVGOwnedTextPathData {
    fn from(value: &SVGTextPathData<'_>) -> Self {
        Self {
            href: value.href.map(Into::into),
            start_offset: value.start_offset,
            text: value.text.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SVGOwnedImageData {
    pub href: Option<SVGOwnedReferenceValue>,
    pub x: Option<layout_api::SVGLengthValue>,
    pub y: Option<layout_api::SVGLengthValue>,
    pub width: Option<layout_api::SVGLengthValue>,
    pub height: Option<layout_api::SVGLengthValue>,
    pub preserve_aspect_ratio: layout_api::SVGPreserveAspectRatioValue,
}

impl SVGOwnedImageData {
    fn borrowed(&self) -> SVGImageData<'_> {
        SVGImageData {
            href: self.href.as_ref().map(SVGOwnedReferenceValue::borrowed),
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            preserve_aspect_ratio: self.preserve_aspect_ratio,
        }
    }
}

impl From<&SVGImageData<'_>> for SVGOwnedImageData {
    fn from(value: &SVGImageData<'_>) -> Self {
        Self {
            href: value.href.map(Into::into),
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
            preserve_aspect_ratio: value.preserve_aspect_ratio,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SVGOwnedCommonData {
    pub element_id: Option<String>,
    pub transform: layout_api::SVGTransformListValue,
}

impl SVGOwnedCommonData {
    fn borrowed(&self) -> SVGCommonData<'_> {
        SVGCommonData {
            element_id: self.element_id.as_deref(),
            transform: self.transform.clone(),
        }
    }
}

impl From<&SVGCommonData<'_>> for SVGOwnedCommonData {
    fn from(value: &SVGCommonData<'_>) -> Self {
        Self {
            element_id: value.element_id.map(str::to_owned),
            transform: value.transform.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum SVGOwnedNodeKind {
    Viewport(SVGViewportData),
    Group,
    Geometry(SVGOwnedGeometryData),
    Text(SVGTextData),
    TSpan(SVGTextData),
    TextPath(SVGOwnedTextPathData),
    Defs,
    Use(SVGOwnedUseData),
    ForeignObject(SVGForeignObjectData),
    Gradient(SVGOwnedGradientData),
    Stop(SVGOwnedStopData),
    ClipPath(SVGClipPathData),
    Mask(SVGMaskData),
    Pattern(SVGOwnedPatternData),
    Filter(SVGFilterData),
    Marker(SVGMarkerData),
    Image(SVGOwnedImageData),
}

impl SVGOwnedNodeKind {
    fn borrowed(&self) -> SVGNodeKind<'_> {
        match self {
            Self::Viewport(data) => SVGNodeKind::Viewport(data.clone()),
            Self::Group => SVGNodeKind::Group,
            Self::Geometry(data) => SVGNodeKind::Geometry(data.borrowed()),
            Self::Text(data) => SVGNodeKind::Text(data.clone()),
            Self::TSpan(data) => SVGNodeKind::TSpan(data.clone()),
            Self::TextPath(data) => SVGNodeKind::TextPath(data.borrowed()),
            Self::Defs => SVGNodeKind::Defs,
            Self::Use(data) => SVGNodeKind::Use(data.borrowed()),
            Self::ForeignObject(data) => SVGNodeKind::ForeignObject(data.clone()),
            Self::Gradient(data) => SVGNodeKind::Gradient(data.borrowed()),
            Self::Stop(data) => SVGNodeKind::Stop(data.borrowed()),
            Self::ClipPath(data) => SVGNodeKind::ClipPath(data.clone()),
            Self::Mask(data) => SVGNodeKind::Mask(data.clone()),
            Self::Pattern(data) => SVGNodeKind::Pattern(data.borrowed()),
            Self::Filter(data) => SVGNodeKind::Filter(data.clone()),
            Self::Marker(data) => SVGNodeKind::Marker(data.clone()),
            Self::Image(data) => SVGNodeKind::Image(data.borrowed()),
        }
    }
}

impl From<&SVGNodeKind<'_>> for SVGOwnedNodeKind {
    fn from(value: &SVGNodeKind<'_>) -> Self {
        match value {
            SVGNodeKind::Viewport(data) => Self::Viewport(data.clone()),
            SVGNodeKind::Group => Self::Group,
            SVGNodeKind::Geometry(data) => Self::Geometry(data.into()),
            SVGNodeKind::Text(data) => Self::Text(data.clone()),
            SVGNodeKind::TSpan(data) => Self::TSpan(data.clone()),
            SVGNodeKind::TextPath(data) => Self::TextPath(data.into()),
            SVGNodeKind::Defs => Self::Defs,
            SVGNodeKind::Use(data) => Self::Use(data.into()),
            SVGNodeKind::ForeignObject(data) => Self::ForeignObject(data.clone()),
            SVGNodeKind::Gradient(data) => Self::Gradient(data.into()),
            SVGNodeKind::Stop(data) => Self::Stop(data.into()),
            SVGNodeKind::ClipPath(data) => Self::ClipPath(data.clone()),
            SVGNodeKind::Mask(data) => Self::Mask(data.clone()),
            SVGNodeKind::Pattern(data) => Self::Pattern(data.into()),
            SVGNodeKind::Filter(data) => Self::Filter(data.clone()),
            SVGNodeKind::Marker(data) => Self::Marker(data.clone()),
            SVGNodeKind::Image(data) => Self::Image(data.into()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SVGNodeData {
    pub common: SVGOwnedCommonData,
    pub node_kind: SVGOwnedNodeKind,
    pub paint: SVGOwnedPaintData,
}

impl SVGNodeData {
    pub fn svg_data(&self) -> SVGElementData<'_> {
        SVGElementData {
            common: self.common.borrowed(),
            node_kind: self.node_kind.borrowed(),
            paint: self.paint.borrowed(),
        }
    }
}

impl From<SVGElementData<'_>> for SVGNodeData {
    fn from(value: SVGElementData<'_>) -> Self {
        Self {
            common: (&value.common).into(),
            node_kind: (&value.node_kind).into(),
            paint: (&value.paint).into(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum SVGTreeChild {
    Node(SVGTreeNode),
    Text(String),
}

#[derive(Clone, Debug)]
pub struct SVGTreeNode {
    pub id: SVGNodeId,
    pub metadata: SVGNodeMetadata,
    pub data: SVGNodeData,
    pub children: Vec<SVGTreeChild>,
}

impl SVGTreeNode {
    pub fn new(id: SVGNodeId, data: SVGNodeData, children: Vec<SVGTreeChild>) -> Self {
        Self {
            id,
            metadata: SVGNodeMetadata::default(),
            data,
            children,
        }
    }

    pub fn with_metadata(mut self, metadata: SVGNodeMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

#[derive(Clone, Debug)]
pub struct SVGStandaloneTree {
    pub root: SVGTreeNode,
}

impl SVGStandaloneTree {
    pub fn new(root: SVGTreeNode) -> Self {
        Self { root }
    }

    pub fn resolve_with_default_style(
        &self,
        computed_style: ServoArc<ComputedValues>,
    ) -> SVGResolvedNode {
        resolve_svg_tree(
            self,
            &|node_id| synthetic_svg_tag(node_id),
            &|_| computed_style.clone(),
        )
    }
}

#[derive(Clone, Debug)]
pub enum SVGResolvedChild {
    Node(SVGResolvedNode),
    Text(String),
}

#[derive(Clone, Debug)]
pub struct SVGResolvedNode {
    pub id: SVGNodeId,
    pub parent_node: Option<OpaqueNode>,
    pub metadata: SVGNodeMetadata,
    pub tag: Tag,
    pub summary: SVGLayoutNodeSummary,
    pub node_data: SVGNodeData,
    pub resolved_style: SVGNodeResolvedStyle,
    pub computed_style: ServoArc<ComputedValues>,
    pub children: Vec<SVGResolvedChild>,
}

impl SVGResolvedNode {
    pub fn svg_data(&self) -> SVGElementData<'_> {
        self.node_data.svg_data()
    }

    pub fn element_children(&self) -> impl Iterator<Item = &SVGResolvedNode> {
        self.children.iter().filter_map(|child| match child {
            SVGResolvedChild::Node(node) => Some(node),
            SVGResolvedChild::Text(_) => None,
        })
    }
}

pub type SVGResolvedNodeMap<'a> = FxHashMap<OpaqueNode, &'a SVGResolvedNode>;

pub fn build_resolved_node_map(root: &SVGResolvedNode) -> SVGResolvedNodeMap<'_> {
    let mut map = FxHashMap::default();
    collect_resolved_node_map(root, &mut map);
    map
}

fn collect_resolved_node_map<'a>(
    node: &'a SVGResolvedNode,
    map: &mut SVGResolvedNodeMap<'a>,
) {
    map.insert(node.tag.node, node);
    for child in node.element_children() {
        collect_resolved_node_map(child, map);
    }
}

pub fn resolve_svg_tree(
    tree: &SVGStandaloneTree,
    tag_for_node: &impl Fn(SVGNodeId) -> Tag,
    computed_style_for_node: &impl Fn(SVGNodeId) -> ServoArc<ComputedValues>,
) -> SVGResolvedNode {
    resolve_svg_tree_node(
        &tree.root,
        None,
        None,
        None,
        tag_for_node,
        computed_style_for_node,
    )
}

fn resolve_svg_tree_node(
    node: &SVGTreeNode,
    parent_tag: Option<Tag>,
    inherited_geometry: Option<&SVGGeometryStyle>,
    inherited_text: Option<&SVGTextStyle>,
    tag_for_node: &impl Fn(SVGNodeId) -> Tag,
    computed_style_for_node: &impl Fn(SVGNodeId) -> ServoArc<ComputedValues>,
) -> SVGResolvedNode {
    let tag = tag_for_node(node.id);
    let computed_style = computed_style_for_node(node.id);
    let svg_data = node.data.svg_data();
    let summary = summarize_node_kind(&svg_data.node_kind);
    let resolved_style = match &svg_data.node_kind {
        SVGNodeKind::Viewport(_) => SVGNodeResolvedStyle::Viewport {
            viewport: resolve_viewport_style(&svg_data, &computed_style),
            geometry: resolve_geometry_style(&svg_data, &computed_style, inherited_geometry),
        },
        SVGNodeKind::Text(_) | SVGNodeKind::TSpan(_) | SVGNodeKind::TextPath(_) => {
            SVGNodeResolvedStyle::Text(resolve_text_style(
                &svg_data,
                &computed_style,
                inherited_text,
            ))
        }
        _ => SVGNodeResolvedStyle::Geometry(resolve_geometry_style(
            &svg_data,
            &computed_style,
            inherited_geometry,
        )),
    };

    let next_inherited_geometry = match &resolved_style {
        SVGNodeResolvedStyle::Viewport { geometry, .. } => Some(geometry.clone()),
        SVGNodeResolvedStyle::Geometry(geometry) => Some(geometry.clone()),
        SVGNodeResolvedStyle::Text(_) => inherited_geometry.cloned(),
    };
    let next_inherited_text = match &resolved_style {
        SVGNodeResolvedStyle::Text(text) => Some(text.clone()),
        _ => inherited_text.cloned(),
    };

    let children = node
        .children
        .iter()
        .map(|child| match child {
            SVGTreeChild::Node(child) => SVGResolvedChild::Node(resolve_svg_tree_node(
                child,
                Some(tag),
                next_inherited_geometry.as_ref(),
                next_inherited_text.as_ref(),
                tag_for_node,
                computed_style_for_node,
            )),
            SVGTreeChild::Text(text) => SVGResolvedChild::Text(text.clone()),
        })
        .collect();

    SVGResolvedNode {
        id: node.id,
        parent_node: parent_tag.map(|tag| tag.node),
        metadata: node.metadata,
        tag,
        summary,
        node_data: node.data.clone(),
        resolved_style,
        computed_style,
        children,
    }
}

pub fn synthetic_svg_tag(node_id: SVGNodeId) -> Tag {
    Tag {
        node: OpaqueNode(node_id.0 + 1),
        pseudo_element_chain: PseudoElementChain::default(),
    }
}
