use app_units::Au;
use servo_arc::Arc as ServoArc;
use style::properties::ComputedValues;

use havi_types::fragment_tree::{SVGPaint, SVGPathData, SVGRect, SVGResourceReferences, SVGStrokeStyle};

use super::dom::{
    snapshot_svg_subtree, SVGDOMNode, SVGDOMTree, SVGLayoutNodeKind, SVGNodeKindOwned,
    SVGNodeResolvedStyle,
};
use super::resources::{
    SVGResourceGraph, SVGResourceGraphNode, SVGResourceReferenceInputs,
};
use super::style::{SVGPaintFallback, SVGResolvedPaint};
use crate::context::LayoutContext;
use crate::fragment_tree::{
    BaseFragment, BaseFragmentInfo, Fragment, FragmentFlags, SVGForeignObjectFragment,
    SVGGroupFragment, SVGImageFragment, SVGPathFragment, SVGTextFragment, SVGViewportFragment,
};
use crate::geom::PhysicalRect;

#[derive(Clone, Debug, Default)]
pub struct SVGViewportLayoutResult {
    pub viewport_rect: Option<SVGRect>,
    pub view_box_rect: Option<SVGRect>,
}

pub(crate) fn snapshot_inline_svg_subtree(
    root: script::layout_dom::ServoThreadSafeLayoutNode<'_>,
    context: &LayoutContext,
) -> Option<SVGDOMTree> {
    snapshot_svg_subtree(root, &context.style_context)
}

pub fn build_inline_svg_fragments(
    tree: &SVGDOMTree,
    base_fragment_info: BaseFragmentInfo,
    outer_style: &ServoArc<ComputedValues>,
    outer_rect: PhysicalRect<Au>,
) -> Vec<Fragment> {
    let resource_graph = SVGResourceGraph::build(&collect_resource_graph_nodes(&tree.root));
    build_svg_node_fragment(
        &tree.root,
        base_fragment_info,
        outer_style.clone(),
        outer_rect,
        Some(resource_graph),
    )
    .into_iter()
    .collect()
}

fn build_svg_node_fragment(
    node: &SVGDOMNode,
    base_fragment_info: BaseFragmentInfo,
    style: ServoArc<ComputedValues>,
    rect: PhysicalRect<Au>,
    resource_graph: Option<SVGResourceGraph>,
) -> Option<Fragment> {
    match (&node.summary.kind, &node.node_kind, &node.resolved_style) {
        (
            SVGLayoutNodeKind::Viewport,
            SVGNodeKindOwned::Viewport(viewport),
            SVGNodeResolvedStyle::Viewport { viewport: viewport_style, .. },
        ) => {
            let children = node
                .children
                .iter()
                .filter_map(|child| {
                    build_svg_child_fragment(child, PhysicalRect::zero(), None)
                })
                .collect();
            let viewport_rect = svg_rect_from_physical_rect(rect);
            let overflow_clip = viewport_style.overflow_hidden.then_some(havi_types::fragment_tree::SVGOverflowClip {
                enabled: true,
                rect: viewport_rect,
            });
            Some(Fragment::SVGViewport(crate::cell::ArcRefCell::new(
                SVGViewportFragment {
                    base: BaseFragment::new(base_fragment_info, style.into(), rect),
                    children,
                    viewport_rect,
                    view_box_rect: viewport.view_box.as_deref().and_then(parse_view_box),
                    local_to_parent_transform: havi_types::fragment_tree::SVGTransform::identity(),
                    overflow_clip,
                    resource_graph,
                },
            )))
        }
        _ => None,
    }
}

fn build_svg_child_fragment(
    node: &SVGDOMNode,
    rect: PhysicalRect<Au>,
    _resource_graph: Option<&SVGResourceGraph>,
) -> Option<Fragment> {
    let base_fragment_info = BaseFragmentInfo {
        tag: Some(node.tag),
        flags: FragmentFlags::empty(),
    };
    let base = BaseFragment::new(base_fragment_info, node.computed_style.clone().into(), rect);
    match (&node.summary.kind, &node.node_kind, &node.resolved_style) {
        (SVGLayoutNodeKind::Group, _, SVGNodeResolvedStyle::Geometry(style))
        | (SVGLayoutNodeKind::Use, _, SVGNodeResolvedStyle::Geometry(style)) => {
            let children = node
                .children
                .iter()
                .filter_map(|child| build_svg_child_fragment(child, PhysicalRect::zero(), None))
                .collect();
            Some(Fragment::SVGGroup(crate::cell::ArcRefCell::new(
                SVGGroupFragment {
                    base,
                    children,
                    local_transform: havi_types::fragment_tree::SVGTransform::identity(),
                    opacity: style.opacity,
                    resources: SVGResourceReferences::default(),
                },
            )))
        }
        (SVGLayoutNodeKind::ForeignObject, _, SVGNodeResolvedStyle::Geometry(_)) => {
            let children = node
                .children
                .iter()
                .filter_map(|child| build_svg_child_fragment(child, PhysicalRect::zero(), None))
                .collect();
            Some(Fragment::SVGForeignObject(crate::cell::ArcRefCell::new(
                SVGForeignObjectFragment {
                    base,
                    children,
                    svg_viewport_rect: SVGRect::zero(),
                    local_transform: havi_types::fragment_tree::SVGTransform::identity(),
                },
            )))
        }
        (SVGLayoutNodeKind::Geometry, _, SVGNodeResolvedStyle::Geometry(style)) => {
            Some(Fragment::SVGPath(crate::cell::ArcRefCell::new(SVGPathFragment {
                base,
                path: SVGPathData {
                    fill_rule: style.fill_rule,
                    commands: Vec::new(),
                },
                object_bounding_box: SVGRect::zero(),
                decorated_bounding_box: SVGRect::zero(),
                local_transform: havi_types::fragment_tree::SVGTransform::identity(),
                fill: convert_resolved_paint(&style.paint.fill),
                stroke: style.paint.stroke.as_ref().map(convert_stroke_style),
                resources: SVGResourceReferences::default(),
            })))
        }
        (SVGLayoutNodeKind::Text, _, SVGNodeResolvedStyle::Text(_style)) => {
            Some(Fragment::SVGText(crate::cell::ArcRefCell::new(SVGTextFragment {
                base,
                glyph_runs: Vec::new(),
                object_bounding_box: SVGRect::zero(),
                decorated_bounding_box: SVGRect::zero(),
                local_transform: havi_types::fragment_tree::SVGTransform::identity(),
                resources: SVGResourceReferences::default(),
            })))
        }
        (SVGLayoutNodeKind::Image, SVGNodeKindOwned::Image(image), SVGNodeResolvedStyle::Geometry(_)) => {
            Some(Fragment::SVGImage(crate::cell::ArcRefCell::new(SVGImageFragment {
                base,
                viewport_rect: SVGRect::zero(),
                local_transform: havi_types::fragment_tree::SVGTransform::identity(),
                href: image.href.clone(),
                resources: SVGResourceReferences::default(),
            })))
        }
        (SVGLayoutNodeKind::Defs, _, _)
        | (SVGLayoutNodeKind::Gradient, _, _)
        | (SVGLayoutNodeKind::Stop, _, _)
        | (SVGLayoutNodeKind::ClipPath, _, _)
        | (SVGLayoutNodeKind::Mask, _, _) => None,
        _ => None,
    }
}

fn collect_resource_graph_nodes(root: &SVGDOMNode) -> Vec<SVGResourceGraphNode> {
    let mut nodes = Vec::new();
    collect_resource_graph_node(root, None, &mut nodes);
    nodes
}

fn collect_resource_graph_node(
    node: &SVGDOMNode,
    parent: Option<style::dom::OpaqueNode>,
    nodes: &mut Vec<SVGResourceGraphNode>,
) {
    let mut graph_node = SVGResourceGraphNode::new(node.tag.node, node.summary.kind);
    graph_node.parent = parent;
    graph_node.element_id = node.common.element_id.clone();
    graph_node.establishes_viewport = node.summary.establishes_viewport;
    graph_node.participates_in_paint = node.summary.participates_in_paint;

    match (&node.node_kind, &node.resolved_style) {
        (SVGNodeKindOwned::Viewport(_), SVGNodeResolvedStyle::Viewport { geometry, .. }) => {
            assign_style_resources(&mut graph_node, geometry);
        }
        (_, SVGNodeResolvedStyle::Geometry(geometry)) => {
            assign_style_resources(&mut graph_node, geometry);
        }
        (_, SVGNodeResolvedStyle::Text(text)) => {
            graph_node.fill_paint_server = paint_server_iri(&text.paint.fill);
            graph_node.stroke_paint_server = text
                .paint
                .stroke
                .as_ref()
                .and_then(|stroke| paint_server_iri(&stroke.paint));
            graph_node.resources = SVGResourceReferenceInputs {
                clip_path: text.resources.clip_path.clone(),
                mask: text.resources.mask.clone(),
                filter: text.resources.filter.clone(),
                marker_start: text.resources.marker_start.clone(),
                marker_mid: text.resources.marker_mid.clone(),
                marker_end: text.resources.marker_end.clone(),
            };
        }
        _ => {}
    }

    graph_node.href = match &node.node_kind {
        SVGNodeKindOwned::Use(data) => data.href.clone(),
        SVGNodeKindOwned::Gradient(data) => match data {
            super::dom::SVGGradientDataOwned::Linear { href, .. }
            | super::dom::SVGGradientDataOwned::Radial { href, .. } => href.clone(),
        },
        _ => None,
    };

    nodes.push(graph_node);
    for child in &node.children {
        collect_resource_graph_node(child, Some(node.tag.node), nodes);
    }
}

fn assign_style_resources(node: &mut SVGResourceGraphNode, style: &super::style::SVGGeometryStyle) {
    node.fill_paint_server = paint_server_iri(&style.paint.fill);
    node.stroke_paint_server = style
        .paint
        .stroke
        .as_ref()
        .and_then(|stroke| paint_server_iri(&stroke.paint));
    node.resources = SVGResourceReferenceInputs {
        clip_path: style.resources.clip_path.clone(),
        mask: style.resources.mask.clone(),
        filter: style.resources.filter.clone(),
        marker_start: style.resources.marker_start.clone(),
        marker_mid: style.resources.marker_mid.clone(),
        marker_end: style.resources.marker_end.clone(),
    };
}

fn paint_server_iri(paint: &SVGResolvedPaint) -> Option<String> {
    match paint {
        SVGResolvedPaint::ResourceReference(reference) => Some(reference.iri.clone()),
        _ => None,
    }
}

fn convert_resolved_paint(paint: &SVGResolvedPaint) -> SVGPaint {
    match paint {
        SVGResolvedPaint::None => SVGPaint::None,
        SVGResolvedPaint::SolidColor(color) => SVGPaint::SolidColor(*color),
        SVGResolvedPaint::ResourceReference(reference) => match &reference.fallback {
            Some(SVGPaintFallback::SolidColor(color)) => SVGPaint::SolidColor(*color),
            Some(SVGPaintFallback::None) | None => SVGPaint::None,
        },
    }
}

fn convert_stroke_style(stroke: &super::style::SVGResolvedStroke) -> SVGStrokeStyle {
    SVGStrokeStyle {
        paint: convert_resolved_paint(&stroke.paint),
        width: stroke.width,
        opacity: stroke.opacity,
        line_cap: stroke.line_cap,
        line_join: stroke.line_join,
        miter_limit: stroke.miter_limit,
        non_scaling: stroke.non_scaling,
    }
}

fn svg_rect_from_physical_rect(rect: PhysicalRect<Au>) -> SVGRect {
    SVGRect::new(
        euclid::point2(rect.origin.x.to_f32_px(), rect.origin.y.to_f32_px()),
        euclid::size2(rect.size.width.to_f32_px(), rect.size.height.to_f32_px()),
    )
}

fn parse_view_box(raw: &str) -> Option<SVGRect> {
    let mut values = raw
        .split(|ch: char| ch == ',' || ch.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<f32>().ok());
    let min_x = values.next()?;
    let min_y = values.next()?;
    let width = values.next()?;
    let height = values.next()?;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    if values.next().is_some() {
        return None;
    }
    Some(SVGRect::new(
        euclid::point2(min_x, min_y),
        euclid::size2(width, height),
    ))
}
