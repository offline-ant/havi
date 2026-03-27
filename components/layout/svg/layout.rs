use std::str::FromStr;

use app_units::Au;
use rustc_hash::FxHashMap;
use servo_arc::Arc as ServoArc;
use style::dom::OpaqueNode;
use style::properties::ComputedValues;

use havi_types::fragment_tree::{
    SVGClipPathResource, SVGColor, SVGCoordinateUnits, SVGGradientKind, SVGGradientResource,
    SVGGradientSpreadMethod, SVGGradientStop, SVGLinearGradient, SVGPaint, SVGPathData, SVGPoint,
    SVGRadialGradient, SVGRect, SVGResourceId, SVGResourceKind,
    SVGStrokeStyle, SVGTransform,
};

use super::dom::{
    snapshot_svg_subtree, SVGClipPathDataOwned, SVGDOMNode, SVGDOMTree, SVGGradientDataOwned,
    SVGLayoutNodeKind, SVGNodeKindOwned, SVGNodeResolvedStyle, SVGStopDataOwned,
};
use super::foreign_object::layout_foreign_object;
use super::path::{
    decorated_bounds, normalize_svg_geometry, parse_svg_length, path_bounds, transform_svg_path_data,
};
use super::resources::{
    SVGResolvedNodeResources, SVGResourceGraph, SVGResourceGraphNode, SVGResourceReferenceInputs,
};
use super::style::{SVGPaintFallback, SVGResolvedPaint};
use super::text::layout_svg_text;
use super::transform::{
    compute_view_box_mapper, parse_svg_transform, then_svg_transform,
};
use super::use_expansion::expand_use_node;
use crate::context::LayoutContext;
use crate::fragment_tree::{
    BaseFragment, BaseFragmentInfo, Fragment, FragmentFlags, Tag, SVGForeignObjectFragment,
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
    let mut resource_graph = SVGResourceGraph::build(&collect_resource_graph_nodes(&tree.root));
    let mut nodes_by_opaque = FxHashMap::default();
    index_svg_nodes(&tree.root, &mut nodes_by_opaque);
    resolve_svg_resources(&tree.root, &nodes_by_opaque, &mut resource_graph);
    build_svg_node_fragment(
        &tree.root,
        base_fragment_info,
        outer_style.clone(),
        outer_rect,
        resource_graph,
        &nodes_by_opaque,
    )
    .into_iter()
    .collect()
}

fn build_svg_node_fragment(
    node: &SVGDOMNode,
    base_fragment_info: BaseFragmentInfo,
    style: ServoArc<ComputedValues>,
    rect: PhysicalRect<Au>,
    resource_graph: SVGResourceGraph,
    nodes_by_opaque: &FxHashMap<OpaqueNode, &SVGDOMNode>,
) -> Option<Fragment> {
    match (&node.summary.kind, &node.node_kind, &node.resolved_style) {
        (
            SVGLayoutNodeKind::Viewport,
            SVGNodeKindOwned::Viewport(viewport),
            SVGNodeResolvedStyle::Viewport { viewport: viewport_style, .. },
        ) => {
            let viewport_rect = svg_rect_from_physical_rect(rect);
            let view_box_rect = viewport.view_box.as_deref().and_then(parse_view_box);
            let mapper = compute_view_box_mapper(
                viewport_rect,
                view_box_rect,
                viewport_style.preserve_aspect_ratio,
            );
            let viewport_transform = then_svg_transform(
                mapper.local_to_parent,
                parse_svg_transform(node.common.transform.as_deref()),
            );
            let overflow_clip = viewport_style.overflow_hidden.then_some(
                havi_types::fragment_tree::SVGOverflowClip {
                    enabled: true,
                    rect: viewport_rect,
                },
            );
            let children = node
                .children
                .iter()
                .filter_map(|child| {
                    build_svg_child_fragment(
                        child,
                        &resource_graph,
                        nodes_by_opaque,
                        None,
                    )
                })
                .collect();
            Some(Fragment::SVGViewport(crate::cell::ArcRefCell::new(
                SVGViewportFragment {
                    base: BaseFragment::new(base_fragment_info, style.into(), rect),
                    children,
                    viewport_rect,
                    view_box_rect,
                    local_to_parent_transform: viewport_transform,
                    overflow_clip,
                    resource_graph: Some(resource_graph),
                },
            )))
        }
        _ => None,
    }
}

fn build_svg_child_fragment(
    node: &SVGDOMNode,
    resource_graph: &SVGResourceGraph,
    nodes_by_opaque: &FxHashMap<OpaqueNode, &SVGDOMNode>,
    tag_override: Option<Tag>,
) -> Option<Fragment> {
    let resolved = resolved_node_resources(resource_graph, node.tag.node);
    let base_fragment_info = BaseFragmentInfo {
        tag: Some(tag_override.unwrap_or(node.tag)),
        flags: FragmentFlags::empty(),
    };

    match (&node.summary.kind, &node.node_kind, &node.resolved_style) {
        (SVGLayoutNodeKind::Group, _, SVGNodeResolvedStyle::Geometry(style)) => {
            let children = node
                .children
                .iter()
                .filter_map(|child| {
                    build_svg_child_fragment(child, resource_graph, nodes_by_opaque, tag_override)
                })
                .collect::<Vec<_>>();
            let rect = union_fragment_rects(&children);
            Some(Fragment::SVGGroup(crate::cell::ArcRefCell::new(
                SVGGroupFragment {
                    base: BaseFragment::new(base_fragment_info, node.computed_style.clone().into(), rect),
                    children,
                    local_transform: parse_svg_transform(node.common.transform.as_deref()),
                    opacity: style.opacity,
                    resources: resolved.resources.clone(),
                },
            )))
        }
        (SVGLayoutNodeKind::Use, _, SVGNodeResolvedStyle::Geometry(style)) => {
            let expansion = expand_use_node(node, nodes_by_opaque, resource_graph);
            let children = expansion
                .referenced_node
                .into_iter()
                .filter_map(|referenced| {
                    build_svg_child_fragment(
                        referenced,
                        resource_graph,
                        nodes_by_opaque,
                        Some(tag_override.unwrap_or(node.tag)),
                    )
                })
                .collect::<Vec<_>>();
            let rect = union_fragment_rects(&children);
            Some(Fragment::SVGGroup(crate::cell::ArcRefCell::new(
                SVGGroupFragment {
                    base: BaseFragment::new(base_fragment_info, node.computed_style.clone().into(), rect),
                    children,
                    local_transform: expansion.instance_transform,
                    opacity: style.opacity,
                    resources: resolved.resources.clone(),
                },
            )))
        }
        (SVGLayoutNodeKind::ForeignObject, _, SVGNodeResolvedStyle::Geometry(_)) => {
            let foreign_object = layout_foreign_object(node);
            let viewport_rect = foreign_object.viewport_rect.unwrap_or_default();
            Some(Fragment::SVGForeignObject(crate::cell::ArcRefCell::new(
                SVGForeignObjectFragment {
                    base: BaseFragment::new(
                        base_fragment_info,
                        node.computed_style.clone().into(),
                        physical_rect_from_svg_rect(viewport_rect),
                    ),
                    children: Vec::new(),
                    svg_viewport_rect: viewport_rect,
                    local_transform: foreign_object.local_transform,
                },
            )))
        }
        (
            SVGLayoutNodeKind::Geometry,
            SVGNodeKindOwned::Geometry(geometry),
            SVGNodeResolvedStyle::Geometry(style),
        ) => {
            let path: SVGPathData = normalize_svg_geometry(geometry, style.fill_rule).into();
            let object_bounding_box = path_bounds(&path).unwrap_or_default();
            let decorated_bounding_box = decorated_bounds(&path, style.paint.stroke.as_ref())
                .unwrap_or(object_bounding_box);
            Some(Fragment::SVGPath(crate::cell::ArcRefCell::new(SVGPathFragment {
                base: BaseFragment::new(
                    base_fragment_info,
                    node.computed_style.clone().into(),
                    physical_rect_from_svg_rect(decorated_bounding_box),
                ),
                path,
                object_bounding_box,
                decorated_bounding_box,
                local_transform: parse_svg_transform(node.common.transform.as_deref()),
                fill: convert_resolved_paint(resource_graph, node.tag.node, &style.paint.fill),
                stroke: style.paint.stroke.as_ref().map(|stroke| {
                    convert_stroke_style(resource_graph, node.tag.node, stroke)
                }),
                resources: resolved.resources.clone(),
            })))
        }
        (SVGLayoutNodeKind::Text, _, SVGNodeResolvedStyle::Text(_text_style)) => {
            let text_layout = layout_svg_text(node);
            Some(Fragment::SVGText(crate::cell::ArcRefCell::new(SVGTextFragment {
                base: BaseFragment::new(
                    base_fragment_info,
                    node.computed_style.clone().into(),
                    physical_rect_from_svg_rect(text_layout.decorated_bounding_box),
                ),
                glyph_runs: text_layout.glyph_runs,
                object_bounding_box: text_layout.object_bounding_box,
                decorated_bounding_box: text_layout.decorated_bounding_box,
                local_transform: parse_svg_transform(node.common.transform.as_deref()),
                resources: resolved.resources.clone(),
            })))
        }
        (SVGLayoutNodeKind::Image, SVGNodeKindOwned::Image(image), SVGNodeResolvedStyle::Geometry(_)) => {
            let viewport_rect = image_viewport(image);
            Some(Fragment::SVGImage(crate::cell::ArcRefCell::new(SVGImageFragment {
                base: BaseFragment::new(
                    base_fragment_info,
                    node.computed_style.clone().into(),
                    physical_rect_from_svg_rect(viewport_rect),
                ),
                viewport_rect,
                local_transform: parse_svg_transform(node.common.transform.as_deref()),
                href: image.href.clone(),
                resources: resolved.resources.clone(),
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

fn resolve_svg_resources(
    root: &SVGDOMNode,
    nodes_by_opaque: &FxHashMap<OpaqueNode, &SVGDOMNode>,
    resource_graph: &mut SVGResourceGraph,
) {
    let mut visiting = Vec::new();
    resolve_svg_resource_node(root, nodes_by_opaque, resource_graph, &mut visiting);
}

fn resolve_svg_resource_node(
    node: &SVGDOMNode,
    nodes_by_opaque: &FxHashMap<OpaqueNode, &SVGDOMNode>,
    resource_graph: &mut SVGResourceGraph,
    visiting: &mut Vec<OpaqueNode>,
) {
    match &node.node_kind {
        SVGNodeKindOwned::Gradient(_) => {
            if let Some(resource_id) = resource_graph.resource_id_for_node(node.tag.node) {
                if let Some(gradient) = resolve_gradient_resource(
                    node,
                    nodes_by_opaque,
                    resource_graph,
                    visiting,
                ) {
                    if let Some(SVGResourceKind::Gradient(resource)) = resource_graph.resource_mut(resource_id) {
                        *resource = gradient;
                    }
                }
            }
        }
        SVGNodeKindOwned::ClipPath(_) => {
            if let Some(resource_id) = resource_graph.resource_id_for_node(node.tag.node) {
                let clip_path = resolve_clip_path_resource(node, nodes_by_opaque, resource_graph);
                if let Some(SVGResourceKind::ClipPath(resource)) = resource_graph.resource_mut(resource_id) {
                    *resource = clip_path;
                }
            }
        }
        SVGNodeKindOwned::Use(_) => {
            if let Some(use_resource_id) = resource_graph
                .node_resources(node.tag.node)
                .and_then(|resolved| resolved.use_instance_source)
            {
                let dependencies = resource_graph
                    .node_resources(node.tag.node)
                    .and_then(|resolved| resolved.referenced_node)
                    .and_then(|source| nodes_by_opaque.get(&source).copied())
                    .map(|source| collect_subtree_resource_dependencies(source, resource_graph))
                    .unwrap_or_default();
                if let Some(SVGResourceKind::UseInstanceSource(resource)) =
                    resource_graph.resource_mut(use_resource_id)
                {
                    resource.source_resource_dependencies = dependencies;
                }
            }
        }
        _ => {}
    }

    for child in &node.children {
        resolve_svg_resource_node(child, nodes_by_opaque, resource_graph, visiting);
    }
}

fn resolve_gradient_resource(
    node: &SVGDOMNode,
    nodes_by_opaque: &FxHashMap<OpaqueNode, &SVGDOMNode>,
    resource_graph: &SVGResourceGraph,
    visiting: &mut Vec<OpaqueNode>,
) -> Option<SVGGradientResource> {
    if visiting.contains(&node.tag.node) {
        return None;
    }
    visiting.push(node.tag.node);

    let template = gradient_template_node(node, nodes_by_opaque, resource_graph)
        .and_then(|template| resolve_gradient_resource(template, nodes_by_opaque, resource_graph, visiting));

    let mut gradient = template.unwrap_or_else(default_gradient_resource);
    match &node.node_kind {
        SVGNodeKindOwned::Gradient(SVGGradientDataOwned::Linear {
            x1,
            y1,
            x2,
            y2,
            gradient_units,
            gradient_transform,
            spread_method,
            ..
        }) => {
            gradient.units = parse_coordinate_units(gradient_units.as_deref())
                .unwrap_or(gradient.units);
            gradient.gradient_transform = parse_svg_transform(gradient_transform.as_deref());
            gradient.spread_method = parse_spread_method(spread_method.as_deref())
                .unwrap_or(gradient.spread_method);
            let linear = match gradient.kind {
                SVGGradientKind::Linear(ref linear) => linear.clone(),
                _ => SVGLinearGradient {
                    start: SVGPoint::new(0.0, 0.0),
                    end: SVGPoint::new(1.0, 0.0),
                },
            };
            gradient.kind = SVGGradientKind::Linear(SVGLinearGradient {
                start: SVGPoint::new(
                    parse_gradient_length(x1.as_deref(), linear.start.x),
                    parse_gradient_length(y1.as_deref(), linear.start.y),
                ),
                end: SVGPoint::new(
                    parse_gradient_length(x2.as_deref(), linear.end.x),
                    parse_gradient_length(y2.as_deref(), linear.end.y),
                ),
            });
        }
        SVGNodeKindOwned::Gradient(SVGGradientDataOwned::Radial {
            cx,
            cy,
            r,
            fx,
            fy,
            gradient_units,
            gradient_transform,
            spread_method,
            ..
        }) => {
            gradient.units = parse_coordinate_units(gradient_units.as_deref())
                .unwrap_or(gradient.units);
            gradient.gradient_transform = parse_svg_transform(gradient_transform.as_deref());
            gradient.spread_method = parse_spread_method(spread_method.as_deref())
                .unwrap_or(gradient.spread_method);
            let radial = match gradient.kind {
                SVGGradientKind::Radial(ref radial) => radial.clone(),
                _ => SVGRadialGradient {
                    center: SVGPoint::new(0.5, 0.5),
                    focal: SVGPoint::new(0.5, 0.5),
                    radius: 0.5,
                },
            };
            let center = SVGPoint::new(
                parse_gradient_length(cx.as_deref(), radial.center.x),
                parse_gradient_length(cy.as_deref(), radial.center.y),
            );
            gradient.kind = SVGGradientKind::Radial(SVGRadialGradient {
                center,
                focal: SVGPoint::new(
                    parse_gradient_length(fx.as_deref(), center.x),
                    parse_gradient_length(fy.as_deref(), center.y),
                ),
                radius: parse_gradient_length(r.as_deref(), radial.radius),
            });
        }
        _ => {}
    }

    let stops = collect_gradient_stops(node);
    if !stops.is_empty() {
        gradient.stops = stops;
    }

    visiting.pop();
    Some(gradient)
}

fn resolve_clip_path_resource(
    node: &SVGDOMNode,
    nodes_by_opaque: &FxHashMap<OpaqueNode, &SVGDOMNode>,
    resource_graph: &SVGResourceGraph,
) -> SVGClipPathResource {
    let (units, transform) = match &node.node_kind {
        SVGNodeKindOwned::ClipPath(SVGClipPathDataOwned { clip_path_units }) => (
            parse_coordinate_units(clip_path_units.as_deref())
                .unwrap_or(SVGCoordinateUnits::UserSpaceOnUse),
            parse_svg_transform(node.common.transform.as_deref()),
        ),
        _ => (SVGCoordinateUnits::UserSpaceOnUse, SVGTransform::identity()),
    };
    SVGClipPathResource {
        units,
        transform,
        paths: collect_clip_paths(node, nodes_by_opaque, resource_graph, SVGTransform::identity()),
    }
}

fn collect_clip_paths(
    node: &SVGDOMNode,
    nodes_by_opaque: &FxHashMap<OpaqueNode, &SVGDOMNode>,
    resource_graph: &SVGResourceGraph,
    inherited_transform: SVGTransform,
) -> Vec<SVGPathData> {
    let node_transform = parse_svg_transform(node.common.transform.as_deref());
    let combined_transform = then_svg_transform(inherited_transform, node_transform);
    match (&node.summary.kind, &node.node_kind, &node.resolved_style) {
        (
            SVGLayoutNodeKind::Geometry,
            SVGNodeKindOwned::Geometry(geometry),
            SVGNodeResolvedStyle::Geometry(style),
        ) => {
            let path: SVGPathData = normalize_svg_geometry(geometry, style.clip_rule).into();
            if path.commands.is_empty() {
                Vec::new()
            } else {
                vec![transform_svg_path_data(&path, combined_transform)]
            }
        }
        (SVGLayoutNodeKind::Group, _, _) | (SVGLayoutNodeKind::ClipPath, _, _) => node
            .children
            .iter()
            .flat_map(|child| {
                collect_clip_paths(child, nodes_by_opaque, resource_graph, combined_transform)
            })
            .collect(),
        (SVGLayoutNodeKind::Use, _, _) => {
            let expansion = expand_use_node(node, nodes_by_opaque, resource_graph);
            let use_transform = then_svg_transform(combined_transform, expansion.instance_transform);
            expansion
                .referenced_node
                .into_iter()
                .flat_map(|referenced| {
                    collect_clip_paths(referenced, nodes_by_opaque, resource_graph, use_transform)
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

fn collect_gradient_stops(node: &SVGDOMNode) -> Vec<SVGGradientStop> {
    node.children
        .iter()
        .filter_map(|child| match &child.node_kind {
            SVGNodeKindOwned::Stop(stop) => Some(resolve_gradient_stop(stop, child)),
            _ => None,
        })
        .collect()
}

fn resolve_gradient_stop(stop: &SVGStopDataOwned, node: &SVGDOMNode) -> SVGGradientStop {
    let color = match &node.resolved_style {
        SVGNodeResolvedStyle::Geometry(style) => style.paint.current_color,
        SVGNodeResolvedStyle::Viewport { geometry, .. } => geometry.paint.current_color,
        SVGNodeResolvedStyle::Text(text) => text.paint.current_color,
    };
    SVGGradientStop {
        offset: parse_stop_offset(stop.offset.as_deref()).unwrap_or(0.0),
        color: stop
            .stop_color
            .as_deref()
            .and_then(parse_svg_color)
            .unwrap_or(color),
        opacity: stop
            .stop_opacity
            .as_deref()
            .and_then(parse_unit_interval)
            .unwrap_or(1.0),
    }
}

fn gradient_template_node<'a>(
    node: &'a SVGDOMNode,
    nodes_by_opaque: &'a FxHashMap<OpaqueNode, &'a SVGDOMNode>,
    resource_graph: &SVGResourceGraph,
) -> Option<&'a SVGDOMNode> {
    let href = match &node.node_kind {
        SVGNodeKindOwned::Gradient(SVGGradientDataOwned::Linear { href, .. })
        | SVGNodeKindOwned::Gradient(SVGGradientDataOwned::Radial { href, .. }) => href.as_deref(),
        _ => None,
    }?;
    let id = href.trim().strip_prefix('#')?;
    let target = resource_graph.node_for_element_id(id)?;
    let target = nodes_by_opaque.get(&target).copied()?;
    matches!(target.node_kind, SVGNodeKindOwned::Gradient(_)).then_some(target)
}

fn collect_subtree_resource_dependencies(
    node: &SVGDOMNode,
    resource_graph: &SVGResourceGraph,
) -> Vec<SVGResourceId> {
    let mut resources = Vec::new();
    collect_subtree_resource_dependencies_into(node, resource_graph, &mut resources);
    resources.sort_by_key(|id| id.0);
    resources.dedup();
    resources
}

fn collect_subtree_resource_dependencies_into(
    node: &SVGDOMNode,
    resource_graph: &SVGResourceGraph,
    resources: &mut Vec<SVGResourceId>,
) {
    let resolved = resolved_node_resources(resource_graph, node.tag.node);
    resources.extend(
        [
            resolved.paint_servers.fill,
            resolved.paint_servers.stroke,
            resolved.resources.clip_path,
            resolved.resources.mask,
            resolved.resources.filter,
            resolved.resources.marker_start,
            resolved.resources.marker_mid,
            resolved.resources.marker_end,
        ]
        .into_iter()
        .flatten(),
    );
    if let Some(resource_id) = resource_graph.resource_id_for_node(node.tag.node) {
        resources.push(resource_id);
    }
    for child in &node.children {
        collect_subtree_resource_dependencies_into(child, resource_graph, resources);
    }
}

fn index_svg_nodes<'a>(node: &'a SVGDOMNode, map: &mut FxHashMap<OpaqueNode, &'a SVGDOMNode>) {
    map.insert(node.tag.node, node);
    for child in &node.children {
        index_svg_nodes(child, map);
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

fn resolved_node_resources(resource_graph: &SVGResourceGraph, node: OpaqueNode) -> SVGResolvedNodeResources {
    resource_graph
        .node_resources(node)
        .cloned()
        .unwrap_or_default()
}

fn convert_resolved_paint(
    resource_graph: &SVGResourceGraph,
    _node: OpaqueNode,
    paint: &SVGResolvedPaint,
) -> SVGPaint {
    match paint {
        SVGResolvedPaint::None => SVGPaint::None,
        SVGResolvedPaint::SolidColor(color) => SVGPaint::SolidColor(*color),
        SVGResolvedPaint::ResourceReference(reference) => {
            let resource_id = reference
                .iri
                .trim()
                .strip_prefix('#')
                .and_then(|id| resource_graph.resource_for_element_id(id))
                .filter(|id| matches!(resource_graph.resource(*id), Some(SVGResourceKind::Gradient(_))));
            if let Some(resource_id) = resource_id {
                SVGPaint::Resource(resource_id)
            } else {
                match &reference.fallback {
                    Some(SVGPaintFallback::SolidColor(color)) => SVGPaint::SolidColor(*color),
                    Some(SVGPaintFallback::None) | None => SVGPaint::None,
                }
            }
        }
    }
}

fn convert_stroke_style(
    resource_graph: &SVGResourceGraph,
    node: OpaqueNode,
    stroke: &super::style::SVGResolvedStroke,
) -> SVGStrokeStyle {
    SVGStrokeStyle {
        paint: convert_resolved_paint(resource_graph, node, &stroke.paint),
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

fn physical_rect_from_svg_rect(rect: SVGRect) -> PhysicalRect<Au> {
    PhysicalRect::new(
        crate::geom::PhysicalPoint::new(
            Au::from_f32_px(rect.origin.x),
            Au::from_f32_px(rect.origin.y),
        ),
        crate::geom::PhysicalSize::new(
            Au::from_f32_px(rect.size.width.max(0.0)),
            Au::from_f32_px(rect.size.height.max(0.0)),
        ),
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

fn image_viewport(image: &super::dom::SVGImageDataOwned) -> SVGRect {
    SVGRect::new(
        euclid::point2(
            parse_svg_length(image.x.as_deref()).unwrap_or(0.0),
            parse_svg_length(image.y.as_deref()).unwrap_or(0.0),
        ),
        euclid::size2(
            parse_svg_length(image.width.as_deref()).unwrap_or(0.0),
            parse_svg_length(image.height.as_deref()).unwrap_or(0.0),
        ),
    )
}

fn union_fragment_rects(fragments: &[Fragment]) -> PhysicalRect<Au> {
    let mut rects = fragments.iter().map(Fragment::content_rect);
    let Some(first) = rects.next() else {
        return PhysicalRect::zero();
    };
    rects.fold(first, |union, rect| union.union(&rect))
}

fn default_gradient_resource() -> SVGGradientResource {
    SVGGradientResource {
        units: SVGCoordinateUnits::ObjectBoundingBox,
        gradient_transform: SVGTransform::identity(),
        spread_method: SVGGradientSpreadMethod::Pad,
        kind: SVGGradientKind::Linear(SVGLinearGradient {
            start: SVGPoint::new(0.0, 0.0),
            end: SVGPoint::new(1.0, 0.0),
        }),
        stops: Vec::new(),
    }
}

fn parse_coordinate_units(raw: Option<&str>) -> Option<SVGCoordinateUnits> {
    match raw?.trim() {
        "userSpaceOnUse" => Some(SVGCoordinateUnits::UserSpaceOnUse),
        "objectBoundingBox" => Some(SVGCoordinateUnits::ObjectBoundingBox),
        _ => None,
    }
}

fn parse_spread_method(raw: Option<&str>) -> Option<SVGGradientSpreadMethod> {
    match raw?.trim() {
        "pad" => Some(SVGGradientSpreadMethod::Pad),
        "reflect" => Some(SVGGradientSpreadMethod::Reflect),
        "repeat" => Some(SVGGradientSpreadMethod::Repeat),
        _ => None,
    }
}

fn parse_gradient_length(raw: Option<&str>, default: f32) -> f32 {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return default;
    };
    let Ok(length) = raw.parse::<svgtypes::Length>() else {
        return default;
    };
    let value = match length.unit {
        svgtypes::LengthUnit::Percent => length.number as f32 / 100.0,
        svgtypes::LengthUnit::None | svgtypes::LengthUnit::Px => length.number as f32,
        svgtypes::LengthUnit::In => (length.number * 96.0) as f32,
        svgtypes::LengthUnit::Cm => (length.number * (96.0 / 2.54)) as f32,
        svgtypes::LengthUnit::Mm => (length.number * (96.0 / 25.4)) as f32,
        svgtypes::LengthUnit::Pt => (length.number * (96.0 / 72.0)) as f32,
        svgtypes::LengthUnit::Pc => (length.number * 16.0) as f32,
        svgtypes::LengthUnit::Em | svgtypes::LengthUnit::Ex => return default,
    };
    if value.is_finite() {
        value
    } else {
        default
    }
}

fn parse_stop_offset(raw: Option<&str>) -> Option<f32> {
    let raw = raw?.trim();
    if let Some(percent) = raw.strip_suffix('%') {
        return percent.parse::<f32>().ok().map(|value| (value / 100.0).clamp(0.0, 1.0));
    }
    raw.parse::<f32>().ok().map(|value| value.clamp(0.0, 1.0))
}

fn parse_svg_color(raw: &str) -> Option<SVGColor> {
    let color = svgtypes::Color::from_str(raw).ok()?;
    Some(SVGColor {
        red: color.red as f32 / 255.0,
        green: color.green as f32 / 255.0,
        blue: color.blue as f32 / 255.0,
        alpha: color.alpha as f32 / 255.0,
    })
}

fn parse_unit_interval(raw: &str) -> Option<f32> {
    raw.trim().parse::<f32>().ok().map(|value| value.clamp(0.0, 1.0))
}
