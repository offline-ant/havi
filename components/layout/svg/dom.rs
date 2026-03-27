use layout_api::wrapper_traits::ThreadSafeLayoutNode;
use layout_api::{
    SVGClipPathData, SVGCommonData, SVGForeignObjectData, SVGGeometryData, SVGGradientData,
    SVGImageData, SVGMaskData, SVGNodeKind, SVGStopData, SVGTextData, SVGUseData,
    SVGViewportData,
};
use script::layout_dom::ServoThreadSafeLayoutNode;
use servo_arc::Arc as ServoArc;
use style::context::SharedStyleContext;
use style::properties::ComputedValues;

use crate::fragment_tree::Tag;

use super::style::{
    resolve_geometry_style, resolve_text_style, resolve_viewport_style, SVGGeometryStyle,
    SVGTextStyle, SVGViewportStyle,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGLayoutNodeKind {
    Viewport,
    Group,
    Geometry,
    Text,
    Defs,
    Use,
    Gradient,
    Stop,
    ClipPath,
    Mask,
    ForeignObject,
    Image,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SVGDOMNodeSummary {
    pub kind: SVGLayoutNodeKind,
    pub establishes_viewport: bool,
    pub participates_in_paint: bool,
}

#[derive(Clone, Debug)]
pub struct SVGDOMTree {
    pub root: SVGDOMNode,
}

#[derive(Clone, Debug)]
pub struct SVGDOMNode {
    pub tag: Tag,
    pub summary: SVGDOMNodeSummary,
    pub common: SVGCommonDataOwned,
    pub node_kind: SVGNodeKindOwned,
    pub resolved_style: SVGNodeResolvedStyle,
    pub computed_style: ServoArc<ComputedValues>,
    pub children: Vec<SVGDOMNode>,
}

#[derive(Clone, Debug)]
pub struct SVGCommonDataOwned {
    pub element_id: Option<String>,
    pub transform: Option<String>,
}

#[derive(Clone, Debug)]
pub enum SVGNodeKindOwned {
    Viewport(SVGViewportDataOwned),
    Group,
    Geometry(SVGGeometryDataOwned),
    Text(SVGTextDataOwned),
    TSpan(SVGTextDataOwned),
    Defs,
    Use(SVGUseDataOwned),
    ForeignObject(SVGForeignObjectDataOwned),
    Gradient(SVGGradientDataOwned),
    Stop(SVGStopDataOwned),
    ClipPath(SVGClipPathDataOwned),
    Mask(SVGMaskDataOwned),
    Image(SVGImageDataOwned),
}

#[derive(Clone, Debug)]
pub enum SVGNodeResolvedStyle {
    Viewport {
        viewport: SVGViewportStyle,
        geometry: SVGGeometryStyle,
    },
    Geometry(SVGGeometryStyle),
    Text(SVGTextStyle),
}

#[derive(Clone, Debug)]
pub struct SVGViewportDataOwned {
    pub width: Option<String>,
    pub height: Option<String>,
    pub view_box: Option<String>,
    pub preserve_aspect_ratio: Option<String>,
    pub overflow: Option<String>,
}

#[derive(Clone, Debug)]
pub enum SVGGeometryDataOwned {
    Path {
        d: Option<String>,
    },
    Rect {
        x: Option<String>,
        y: Option<String>,
        width: Option<String>,
        height: Option<String>,
        rx: Option<String>,
        ry: Option<String>,
    },
    Circle {
        cx: Option<String>,
        cy: Option<String>,
        r: Option<String>,
    },
    Ellipse {
        cx: Option<String>,
        cy: Option<String>,
        rx: Option<String>,
        ry: Option<String>,
    },
    Line {
        x1: Option<String>,
        y1: Option<String>,
        x2: Option<String>,
        y2: Option<String>,
    },
    Polyline {
        points: Option<String>,
    },
    Polygon {
        points: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct SVGTextDataOwned {
    pub x: Option<String>,
    pub y: Option<String>,
    pub dx: Option<String>,
    pub dy: Option<String>,
    pub rotate: Option<String>,
    pub text_length: Option<String>,
    pub length_adjust: Option<String>,
    pub text_anchor: Option<String>,
    pub alignment_baseline: Option<String>,
    pub dominant_baseline: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SVGUseDataOwned {
    pub href: Option<String>,
    pub x: Option<String>,
    pub y: Option<String>,
    pub width: Option<String>,
    pub height: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SVGForeignObjectDataOwned {
    pub x: Option<String>,
    pub y: Option<String>,
    pub width: Option<String>,
    pub height: Option<String>,
}

#[derive(Clone, Debug)]
pub enum SVGGradientDataOwned {
    Linear {
        href: Option<String>,
        x1: Option<String>,
        y1: Option<String>,
        x2: Option<String>,
        y2: Option<String>,
        gradient_units: Option<String>,
        gradient_transform: Option<String>,
        spread_method: Option<String>,
    },
    Radial {
        href: Option<String>,
        cx: Option<String>,
        cy: Option<String>,
        r: Option<String>,
        fx: Option<String>,
        fy: Option<String>,
        fr: Option<String>,
        gradient_units: Option<String>,
        gradient_transform: Option<String>,
        spread_method: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct SVGStopDataOwned {
    pub offset: Option<String>,
    pub stop_color: Option<String>,
    pub stop_opacity: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SVGClipPathDataOwned {
    pub clip_path_units: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SVGMaskDataOwned {
    pub x: Option<String>,
    pub y: Option<String>,
    pub width: Option<String>,
    pub height: Option<String>,
    pub mask_units: Option<String>,
    pub mask_content_units: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SVGImageDataOwned {
    pub href: Option<String>,
    pub x: Option<String>,
    pub y: Option<String>,
    pub width: Option<String>,
    pub height: Option<String>,
    pub preserve_aspect_ratio: Option<String>,
}

pub fn snapshot_svg_subtree(
    root: ServoThreadSafeLayoutNode<'_>,
    context: &SharedStyleContext,
) -> Option<SVGDOMTree> {
    let root = snapshot_svg_node(root, context, None, None)?;
    if root.summary.kind != SVGLayoutNodeKind::Viewport {
        return None;
    }
    Some(SVGDOMTree { root })
}

fn snapshot_svg_node(
    node: ServoThreadSafeLayoutNode<'_>,
    context: &SharedStyleContext,
    inherited_geometry: Option<&SVGGeometryStyle>,
    inherited_text: Option<&SVGTextStyle>,
) -> Option<SVGDOMNode> {
    let svg_data = node.svg_data()?;
    let tag = Tag::from(node);
    let computed_style = node.style(context);
    let summary = summarize_node_kind(&svg_data.node_kind);
    let resolved_style = match &svg_data.node_kind {
        SVGNodeKind::Viewport(_) => SVGNodeResolvedStyle::Viewport {
            viewport: resolve_viewport_style(&svg_data, &computed_style),
            geometry: resolve_geometry_style(&svg_data, &computed_style, inherited_geometry),
        },
        SVGNodeKind::Text(_) | SVGNodeKind::TSpan(_) => {
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
    let node_kind = SVGNodeKindOwned::from_svg_data(&svg_data.node_kind);
    let common = SVGCommonDataOwned::from_svg_data(&svg_data.common);

    let next_geometry = match &resolved_style {
        SVGNodeResolvedStyle::Viewport { geometry, .. } => Some(geometry),
        SVGNodeResolvedStyle::Geometry(geometry) => Some(geometry),
        SVGNodeResolvedStyle::Text(_) => inherited_geometry,
    };
    let next_text = match &resolved_style {
        SVGNodeResolvedStyle::Text(text) => Some(text),
        _ => inherited_text,
    };

    let children = node
        .children()
        .filter_map(|child| snapshot_svg_node(child, context, next_geometry, next_text))
        .collect();

    Some(SVGDOMNode {
        tag,
        summary,
        common,
        node_kind,
        resolved_style,
        computed_style,
        children,
    })
}

fn summarize_node_kind(node_kind: &SVGNodeKind<'_>) -> SVGDOMNodeSummary {
    match node_kind {
        SVGNodeKind::Viewport(_) => SVGDOMNodeSummary {
            kind: SVGLayoutNodeKind::Viewport,
            establishes_viewport: true,
            participates_in_paint: true,
        },
        SVGNodeKind::Group => SVGDOMNodeSummary {
            kind: SVGLayoutNodeKind::Group,
            establishes_viewport: false,
            participates_in_paint: true,
        },
        SVGNodeKind::Geometry(_) => SVGDOMNodeSummary {
            kind: SVGLayoutNodeKind::Geometry,
            establishes_viewport: false,
            participates_in_paint: true,
        },
        SVGNodeKind::Text(_) | SVGNodeKind::TSpan(_) => SVGDOMNodeSummary {
            kind: SVGLayoutNodeKind::Text,
            establishes_viewport: false,
            participates_in_paint: true,
        },
        SVGNodeKind::Defs => SVGDOMNodeSummary {
            kind: SVGLayoutNodeKind::Defs,
            establishes_viewport: false,
            participates_in_paint: false,
        },
        SVGNodeKind::Use(_) => SVGDOMNodeSummary {
            kind: SVGLayoutNodeKind::Use,
            establishes_viewport: false,
            participates_in_paint: true,
        },
        SVGNodeKind::Gradient(_) => SVGDOMNodeSummary {
            kind: SVGLayoutNodeKind::Gradient,
            establishes_viewport: false,
            participates_in_paint: false,
        },
        SVGNodeKind::Stop(_) => SVGDOMNodeSummary {
            kind: SVGLayoutNodeKind::Stop,
            establishes_viewport: false,
            participates_in_paint: false,
        },
        SVGNodeKind::ClipPath(_) => SVGDOMNodeSummary {
            kind: SVGLayoutNodeKind::ClipPath,
            establishes_viewport: false,
            participates_in_paint: false,
        },
        SVGNodeKind::Mask(_) => SVGDOMNodeSummary {
            kind: SVGLayoutNodeKind::Mask,
            establishes_viewport: false,
            participates_in_paint: false,
        },
        SVGNodeKind::ForeignObject(_) => SVGDOMNodeSummary {
            kind: SVGLayoutNodeKind::ForeignObject,
            establishes_viewport: false,
            participates_in_paint: true,
        },
        SVGNodeKind::Image(_) => SVGDOMNodeSummary {
            kind: SVGLayoutNodeKind::Image,
            establishes_viewport: false,
            participates_in_paint: true,
        },
    }
}

impl SVGCommonDataOwned {
    fn from_svg_data(data: &SVGCommonData<'_>) -> Self {
        Self {
            element_id: data.element_id.map(ToOwned::to_owned),
            transform: data.transform.map(ToOwned::to_owned),
        }
    }
}

impl SVGNodeKindOwned {
    fn from_svg_data(data: &SVGNodeKind<'_>) -> Self {
        match data {
            SVGNodeKind::Viewport(data) => Self::Viewport(SVGViewportDataOwned::from_svg_data(data)),
            SVGNodeKind::Group => Self::Group,
            SVGNodeKind::Geometry(data) => Self::Geometry(SVGGeometryDataOwned::from_svg_data(data)),
            SVGNodeKind::Text(data) => Self::Text(SVGTextDataOwned::from_svg_data(data)),
            SVGNodeKind::TSpan(data) => Self::TSpan(SVGTextDataOwned::from_svg_data(data)),
            SVGNodeKind::Defs => Self::Defs,
            SVGNodeKind::Use(data) => Self::Use(SVGUseDataOwned::from_svg_data(data)),
            SVGNodeKind::ForeignObject(data) => {
                Self::ForeignObject(SVGForeignObjectDataOwned::from_svg_data(data))
            }
            SVGNodeKind::Gradient(data) => Self::Gradient(SVGGradientDataOwned::from_svg_data(data)),
            SVGNodeKind::Stop(data) => Self::Stop(SVGStopDataOwned::from_svg_data(data)),
            SVGNodeKind::ClipPath(data) => Self::ClipPath(SVGClipPathDataOwned::from_svg_data(data)),
            SVGNodeKind::Mask(data) => Self::Mask(SVGMaskDataOwned::from_svg_data(data)),
            SVGNodeKind::Image(data) => Self::Image(SVGImageDataOwned::from_svg_data(data)),
        }
    }
}

impl SVGViewportDataOwned {
    fn from_svg_data(data: &SVGViewportData<'_>) -> Self {
        Self {
            width: data.width.map(ToOwned::to_owned),
            height: data.height.map(ToOwned::to_owned),
            view_box: data.view_box.map(ToOwned::to_owned),
            preserve_aspect_ratio: data.preserve_aspect_ratio.map(ToOwned::to_owned),
            overflow: data.overflow.map(ToOwned::to_owned),
        }
    }
}

impl SVGGeometryDataOwned {
    fn from_svg_data(data: &SVGGeometryData<'_>) -> Self {
        match data {
            SVGGeometryData::Path { d } => Self::Path {
                d: d.map(ToOwned::to_owned),
            },
            SVGGeometryData::Rect {
                x,
                y,
                width,
                height,
                rx,
                ry,
            } => Self::Rect {
                x: x.map(ToOwned::to_owned),
                y: y.map(ToOwned::to_owned),
                width: width.map(ToOwned::to_owned),
                height: height.map(ToOwned::to_owned),
                rx: rx.map(ToOwned::to_owned),
                ry: ry.map(ToOwned::to_owned),
            },
            SVGGeometryData::Circle { cx, cy, r } => Self::Circle {
                cx: cx.map(ToOwned::to_owned),
                cy: cy.map(ToOwned::to_owned),
                r: r.map(ToOwned::to_owned),
            },
            SVGGeometryData::Ellipse { cx, cy, rx, ry } => Self::Ellipse {
                cx: cx.map(ToOwned::to_owned),
                cy: cy.map(ToOwned::to_owned),
                rx: rx.map(ToOwned::to_owned),
                ry: ry.map(ToOwned::to_owned),
            },
            SVGGeometryData::Line { x1, y1, x2, y2 } => Self::Line {
                x1: x1.map(ToOwned::to_owned),
                y1: y1.map(ToOwned::to_owned),
                x2: x2.map(ToOwned::to_owned),
                y2: y2.map(ToOwned::to_owned),
            },
            SVGGeometryData::Polyline { points } => Self::Polyline {
                points: points.map(ToOwned::to_owned),
            },
            SVGGeometryData::Polygon { points } => Self::Polygon {
                points: points.map(ToOwned::to_owned),
            },
        }
    }
}

impl SVGTextDataOwned {
    fn from_svg_data(data: &SVGTextData<'_>) -> Self {
        Self {
            x: data.x.map(ToOwned::to_owned),
            y: data.y.map(ToOwned::to_owned),
            dx: data.dx.map(ToOwned::to_owned),
            dy: data.dy.map(ToOwned::to_owned),
            rotate: data.rotate.map(ToOwned::to_owned),
            text_length: data.text_length.map(ToOwned::to_owned),
            length_adjust: data.length_adjust.map(ToOwned::to_owned),
            text_anchor: data.text_anchor.map(ToOwned::to_owned),
            alignment_baseline: data.alignment_baseline.map(ToOwned::to_owned),
            dominant_baseline: data.dominant_baseline.map(ToOwned::to_owned),
        }
    }
}

impl SVGUseDataOwned {
    fn from_svg_data(data: &SVGUseData<'_>) -> Self {
        Self {
            href: data.href.map(ToOwned::to_owned),
            x: data.x.map(ToOwned::to_owned),
            y: data.y.map(ToOwned::to_owned),
            width: data.width.map(ToOwned::to_owned),
            height: data.height.map(ToOwned::to_owned),
        }
    }
}

impl SVGForeignObjectDataOwned {
    fn from_svg_data(data: &SVGForeignObjectData<'_>) -> Self {
        Self {
            x: data.x.map(ToOwned::to_owned),
            y: data.y.map(ToOwned::to_owned),
            width: data.width.map(ToOwned::to_owned),
            height: data.height.map(ToOwned::to_owned),
        }
    }
}

impl SVGGradientDataOwned {
    fn from_svg_data(data: &SVGGradientData<'_>) -> Self {
        match data {
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
                href: href.map(ToOwned::to_owned),
                x1: x1.map(ToOwned::to_owned),
                y1: y1.map(ToOwned::to_owned),
                x2: x2.map(ToOwned::to_owned),
                y2: y2.map(ToOwned::to_owned),
                gradient_units: gradient_units.map(ToOwned::to_owned),
                gradient_transform: gradient_transform.map(ToOwned::to_owned),
                spread_method: spread_method.map(ToOwned::to_owned),
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
                href: href.map(ToOwned::to_owned),
                cx: cx.map(ToOwned::to_owned),
                cy: cy.map(ToOwned::to_owned),
                r: r.map(ToOwned::to_owned),
                fx: fx.map(ToOwned::to_owned),
                fy: fy.map(ToOwned::to_owned),
                fr: fr.map(ToOwned::to_owned),
                gradient_units: gradient_units.map(ToOwned::to_owned),
                gradient_transform: gradient_transform.map(ToOwned::to_owned),
                spread_method: spread_method.map(ToOwned::to_owned),
            },
        }
    }
}

impl SVGStopDataOwned {
    fn from_svg_data(data: &SVGStopData<'_>) -> Self {
        Self {
            offset: data.offset.map(ToOwned::to_owned),
            stop_color: data.stop_color.map(ToOwned::to_owned),
            stop_opacity: data.stop_opacity.map(ToOwned::to_owned),
        }
    }
}

impl SVGClipPathDataOwned {
    fn from_svg_data(data: &SVGClipPathData<'_>) -> Self {
        Self {
            clip_path_units: data.clip_path_units.map(ToOwned::to_owned),
        }
    }
}

impl SVGMaskDataOwned {
    fn from_svg_data(data: &SVGMaskData<'_>) -> Self {
        Self {
            x: data.x.map(ToOwned::to_owned),
            y: data.y.map(ToOwned::to_owned),
            width: data.width.map(ToOwned::to_owned),
            height: data.height.map(ToOwned::to_owned),
            mask_units: data.mask_units.map(ToOwned::to_owned),
            mask_content_units: data.mask_content_units.map(ToOwned::to_owned),
        }
    }
}

impl SVGImageDataOwned {
    fn from_svg_data(data: &SVGImageData<'_>) -> Self {
        Self {
            href: data.href.map(ToOwned::to_owned),
            x: data.x.map(ToOwned::to_owned),
            y: data.y.map(ToOwned::to_owned),
            width: data.width.map(ToOwned::to_owned),
            height: data.height.map(ToOwned::to_owned),
            preserve_aspect_ratio: data.preserve_aspect_ratio.map(ToOwned::to_owned),
        }
    }
}
