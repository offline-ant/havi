use std::str::FromStr;

use app_units::Au;
use html5ever::{local_name, ns};
use malloc_size_of_derive::MallocSizeOf;
use rustc_hash::FxHashMap;
use servo_arc::Arc as ServoArc;
use style::computed_values::object_fit::T as ObjectFit;
use style::context::SharedStyleContext;
use style::dom::OpaqueNode;
use style::logical_geometry::{Direction, WritingMode};
use style::properties::ComputedValues;
use style::values::CSSFloat;

use havi_types::fragment_tree::{
    SVGClipPathResource, SVGColor, SVGCoordinateUnits, SVGFragmentIdentity, SVGGradientKind,
    SVGGradientResource, SVGGradientSpreadMethod, SVGGradientStop, SVGLinearGradient, SVGPaint,
    SVGPathData, SVGPoint, SVGRect, SVGResourceId, SVGResourceKind, SVGStrokeStyle,
    SVGTransform, SVGUseInstanceChain, SVGRadialGradient, Tag as PublishedTag,
};
use layout_api::wrapper_traits::{ThreadSafeLayoutElement, ThreadSafeLayoutNode};
use layout_api::{SVGElementData, SVGGradientData, SVGNodeKind};
use script::layout_dom::{ServoLayoutNode, ServoThreadSafeLayoutNode};

use super::dom::{
    resolve_svg_child_node, resolve_svg_node, SVGLayoutNodeKind, SVGNodeResolvedStyle,
    SVGResolvedNode,
};
use super::foreign_object::{layout_foreign_object, layout_foreign_object_children};
use super::path::{
    decorated_bounds, normalize_svg_geometry, parse_svg_length, path_bounds, transform_svg_path_data,
};
use super::resources::{
    SVGResolvedNodeResources, SVGResourceGraph, SVGResourceGraphNode, SVGResourceReferenceInputs,
};
use super::style::{SVGPaintFallback, SVGResolvedPaint};
use super::text::layout_svg_text;
use super::transform::{compute_view_box_mapper, parse_svg_transform, then_svg_transform};
use super::use_expansion::expand_use_node;
use crate::context::LayoutContext;
use crate::dom::NodeExt;
use crate::fragment_tree::{
    BaseFragment, BaseFragmentInfo, CollapsedBlockMargins, Fragment, FragmentFlags, Tag,
    SVGForeignObjectFragment, SVGGroupFragment, SVGImageFragment, SVGPathFragment,
    SVGTextFragment, SVGViewportFragment,
};
use crate::geom::{LogicalVec2, PhysicalPoint, PhysicalRect, PhysicalSize};
use crate::layout_box_base::{CacheableLayoutResult, LayoutBoxBase};
use crate::positioned::PositioningContext;
use crate::sizing::{
    ComputeInlineContentSizes, InlineContentSizesResult, LazySize, SizeConstraint,
};
use crate::style_ext::{AspectRatio, Clamp, ComputedValuesExt};
use crate::{ConstraintSpace, ContainingBlock};

#[derive(Debug, Default, MallocSizeOf)]
struct SVGRootIntrinsicSizes {
    width: Option<Au>,
    height: Option<Au>,
    ratio: Option<CSSFloat>,
}

#[derive(Debug, MallocSizeOf)]
pub(crate) struct SVGRootContents {
    root_node_id: usize,
    intrinsic_size: SVGRootIntrinsicSizes,
}

impl SVGRootContents {
    pub(crate) fn for_element(
        node: ServoThreadSafeLayoutNode<'_>,
        _context: &LayoutContext,
    ) -> Option<Self> {
        let svg_data = node.as_svg().filter(|data| data.viewport().is_some())?;
        Some(Self {
            root_node_id: node.unsafe_get().trusted_node_id(),
            intrinsic_size: intrinsic_svg_root_sizes(&svg_data),
        })
    }

    fn content_size(
        &self,
        axis: Direction,
        preferred_aspect_ratio: Option<AspectRatio>,
        get_size_in_opposite_axis: &dyn Fn() -> SizeConstraint,
        get_fallback_size: &dyn Fn() -> Au,
    ) -> Au {
        let Some(ratio) = preferred_aspect_ratio else {
            return get_fallback_size();
        };
        let transfer = |size| ratio.compute_dependent_size(axis, size);
        match get_size_in_opposite_axis() {
            SizeConstraint::Definite(size) => transfer(size),
            SizeConstraint::MinMax(min_size, max_size) => get_fallback_size()
                .clamp_between_extremums(transfer(min_size), max_size.map(transfer)),
        }
    }

    fn calculate_fragment_rect(
        &self,
        style: &ServoArc<ComputedValues>,
        size: PhysicalSize<Au>,
    ) -> PhysicalRect<Au> {
        let natural_size = PhysicalSize::new(
            self.intrinsic_size.width.unwrap_or(size.width),
            self.intrinsic_size.height.unwrap_or(size.height),
        );

        let object_fit_size = self.intrinsic_size.ratio.map_or(size, |width_over_height| {
            let preserve_aspect_ratio_with_comparison =
                |size: PhysicalSize<Au>, comparison: fn(&Au, &Au) -> bool| {
                    let candidate_width = size.height.scale_by(width_over_height);
                    if comparison(&candidate_width, &size.width) {
                        return PhysicalSize::new(candidate_width, size.height);
                    }

                    let candidate_height = size.width.scale_by(1. / width_over_height);
                    debug_assert!(comparison(&candidate_height, &size.height));
                    PhysicalSize::new(size.width, candidate_height)
                };

            match style.clone_object_fit() {
                ObjectFit::Fill => size,
                ObjectFit::Contain => preserve_aspect_ratio_with_comparison(size, PartialOrd::le),
                ObjectFit::Cover => preserve_aspect_ratio_with_comparison(size, PartialOrd::ge),
                ObjectFit::None => natural_size,
                ObjectFit::ScaleDown => {
                    preserve_aspect_ratio_with_comparison(size.min(natural_size), PartialOrd::le)
                },
            }
        });

        let object_position = style.clone_object_position();
        let horizontal_position = object_position
            .horizontal
            .to_used_value(size.width - object_fit_size.width);
        let vertical_position = object_position
            .vertical
            .to_used_value(size.height - object_fit_size.height);

        PhysicalRect::new(
            PhysicalPoint::new(horizontal_position, vertical_position),
            object_fit_size,
        )
    }

    pub(crate) fn preferred_aspect_ratio(
        &self,
        style: &ComputedValues,
        padding_border_sums: &LogicalVec2<Au>,
    ) -> Option<AspectRatio> {
        style.preferred_aspect_ratio(self.intrinsic_size.ratio, padding_border_sums)
    }

    pub(crate) fn fallback_inline_size(&self, writing_mode: WritingMode) -> Au {
        if writing_mode.is_horizontal() {
            self.intrinsic_size.width.unwrap_or_else(|| Au::from_px(300))
        } else {
            self.intrinsic_size.height.unwrap_or_else(|| Au::from_px(150))
        }
    }

    pub(crate) fn fallback_block_size(&self, writing_mode: WritingMode) -> Au {
        if writing_mode.is_horizontal() {
            self.intrinsic_size.height.unwrap_or_else(|| Au::from_px(150))
        } else {
            self.intrinsic_size.width.unwrap_or_else(|| Au::from_px(300))
        }
    }

    pub(crate) fn logical_natural_sizes(
        &self,
        writing_mode: WritingMode,
    ) -> LogicalVec2<Option<Au>> {
        if writing_mode.is_horizontal() {
            LogicalVec2 {
                inline: self.intrinsic_size.width,
                block: self.intrinsic_size.height,
            }
        } else {
            LogicalVec2 {
                inline: self.intrinsic_size.height,
                block: self.intrinsic_size.width,
            }
        }
    }

    pub(crate) fn layout(
        &self,
        layout_context: &LayoutContext,
        positioning_context: &mut PositioningContext,
        containing_block_for_children: &ContainingBlock,
        preferred_aspect_ratio: Option<AspectRatio>,
        base: &LayoutBoxBase,
        lazy_block_size: &LazySize,
    ) -> CacheableLayoutResult {
        let writing_mode = base.style.writing_mode;
        let inline_size = containing_block_for_children.size.inline;
        let content_block_size = self.content_size(
            Direction::Block,
            preferred_aspect_ratio,
            &|| SizeConstraint::Definite(inline_size),
            &|| self.fallback_block_size(writing_mode),
        );
        let size = LogicalVec2 {
            inline: inline_size,
            block: lazy_block_size.resolve(|| content_block_size),
        }
        .to_physical_size(writing_mode);
        let rect = self.calculate_fragment_rect(&base.style, size);
        let root_node = ServoThreadSafeLayoutNode::new(ServoLayoutNode::from_trusted_node_id(
            self.root_node_id,
        ));
        let fragments = build_svg_root_fragment(
            root_node,
            layout_context,
            positioning_context,
            base.base_fragment_info,
            &base.style,
            rect,
        )
        .into_iter()
        .collect();
        CacheableLayoutResult {
            baselines: Default::default(),
            collapsible_margins_in_children: CollapsedBlockMargins::zero(),
            content_block_size,
            content_inline_size_for_table: None,
            depends_on_block_constraints: true,
            fragments,
            specific_layout_info: None,
        }
    }
}

impl ComputeInlineContentSizes for SVGRootContents {
    fn compute_inline_content_sizes(
        &self,
        _: &LayoutContext,
        constraint_space: &ConstraintSpace,
    ) -> InlineContentSizesResult {
        let inline_content_size = self.content_size(
            Direction::Inline,
            constraint_space.preferred_aspect_ratio,
            &|| constraint_space.block_size,
            &|| self.fallback_inline_size(constraint_space.style.writing_mode),
        );
        InlineContentSizesResult {
            sizes: inline_content_size.into(),
            depends_on_block_constraints: constraint_space.preferred_aspect_ratio.is_some(),
        }
    }
}

fn intrinsic_svg_root_sizes(svg_data: &SVGElementData<'_>) -> SVGRootIntrinsicSizes {
    let viewport = svg_data
        .viewport()
        .expect("outer SVG sizing only applies to viewport nodes");
    let width = parse_svg_length(viewport.width).filter(|width| *width >= 0.0);
    let height = parse_svg_length(viewport.height).filter(|height| *height >= 0.0);

    let ratio = match (width, height) {
        (Some(width), Some(height)) if width > 0.0 && height > 0.0 => Some(width / height),
        _ => viewport.ratio_from_view_box(),
    };

    SVGRootIntrinsicSizes {
        width: width.map(Au::from_f32_px),
        height: height.map(Au::from_f32_px),
        ratio,
    }
}

pub(crate) fn build_svg_root_fragment(
    root: ServoThreadSafeLayoutNode<'_>,
    layout_context: &LayoutContext,
    positioning_context: &mut PositioningContext,
    base_fragment_info: BaseFragmentInfo,
    outer_style: &ServoArc<ComputedValues>,
    outer_rect: PhysicalRect<Au>,
) -> Option<Fragment> {
    let style_context = &layout_context.style_context;
    let root = resolve_svg_node(root, style_context)?;
    if root.summary.kind != SVGLayoutNodeKind::Viewport {
        return None;
    }
    let (mut resource_graph, nodes_by_opaque) = build_svg_resource_graph(&root, style_context);
    resolve_svg_resources(&root, style_context, &nodes_by_opaque, &mut resource_graph);
    build_svg_node_fragment(
        &root,
        layout_context,
        positioning_context,
        base_fragment_info,
        outer_style.clone(),
        outer_rect,
        resource_graph,
        &nodes_by_opaque,
    )
}

type SVGNodeMap<'dom> = FxHashMap<OpaqueNode, ServoThreadSafeLayoutNode<'dom>>;

#[derive(Clone, Debug, Default)]
struct SVGFragmentIdentityContext {
    current_instance_owner_tag: Option<Tag>,
    instance_chain: Option<Box<SVGUseInstanceChain>>,
}

impl SVGFragmentIdentityContext {
    fn for_expanded_use(self, use_tag: Tag) -> Self {
        Self {
            current_instance_owner_tag: Some(use_tag),
            instance_chain: Some(Box::new(SVGUseInstanceChain {
                owner_tag: published_tag(use_tag),
                parent: self.instance_chain,
            })),
        }
    }

    fn base_fragment_info(&self, source_tag: Tag) -> BaseFragmentInfo {
        BaseFragmentInfo {
            tag: Some(self.publication_tag(source_tag)),
            flags: FragmentFlags::empty(),
        }
    }

    fn fragment_identity(&self, source_tag: Tag) -> SVGFragmentIdentity {
        SVGFragmentIdentity {
            source_tag: published_tag(source_tag),
            instance_chain: self.instance_chain.clone(),
        }
    }

    fn publication_tag(&self, source_tag: Tag) -> Tag {
        self.current_instance_owner_tag.unwrap_or(source_tag)
    }
}

fn published_tag(tag: Tag) -> PublishedTag {
    PublishedTag {
        node: tag.node,
        pseudo: tag.pseudo_element_chain.primary,
    }
}

fn build_svg_node_fragment(
    node: &SVGResolvedNode<'_>,
    layout_context: &LayoutContext,
    positioning_context: &mut PositioningContext,
    base_fragment_info: BaseFragmentInfo,
    style: ServoArc<ComputedValues>,
    rect: PhysicalRect<Au>,
    resource_graph: SVGResourceGraph,
    nodes_by_opaque: &SVGNodeMap<'_>,
) -> Option<Fragment> {
    match (&node.summary.kind, &node.svg_data.node_kind, &node.resolved_style) {
        (
            SVGLayoutNodeKind::Viewport,
            SVGNodeKind::Viewport(viewport),
            SVGNodeResolvedStyle::Viewport { viewport: viewport_style, .. },
        ) => {
            let viewport_rect = svg_rect_from_physical_rect(rect);
            let view_box_rect = viewport.view_box.and_then(parse_view_box);
            let mapper = compute_view_box_mapper(
                viewport_rect,
                view_box_rect,
                viewport_style.preserve_aspect_ratio,
            );
            let viewport_transform = then_svg_transform(
                mapper.local_to_parent,
                parse_svg_transform(node.svg_data.common.transform),
            );
            let overflow_clip = viewport_style.overflow_hidden.then_some(
                havi_types::fragment_tree::SVGOverflowClip {
                    enabled: true,
                    rect: viewport_rect,
                },
            );
            let children = build_svg_children(
                node,
                layout_context,
                positioning_context,
                &resource_graph,
                nodes_by_opaque,
                SVGFragmentIdentityContext::default(),
            );
            Some(Fragment::SVGViewport(crate::cell::ArcRefCell::new(
                SVGViewportFragment {
                    base: BaseFragment::new(base_fragment_info, style.into(), rect),
                    identity: SVGFragmentIdentityContext::default().fragment_identity(node.tag),
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

fn build_svg_children(
    node: &SVGResolvedNode<'_>,
    layout_context: &LayoutContext,
    positioning_context: &mut PositioningContext,
    resource_graph: &SVGResourceGraph,
    nodes_by_opaque: &SVGNodeMap<'_>,
    identity_context: SVGFragmentIdentityContext,
) -> Vec<Fragment> {
    let mut children = Vec::new();
    for child in node.node.children() {
        let Some(child) = resolve_svg_child_node(child, &layout_context.style_context, node) else {
            continue;
        };
        let Some(fragment) = build_svg_child_fragment(
            &child,
            layout_context,
            positioning_context,
            resource_graph,
            nodes_by_opaque,
            identity_context.clone(),
        ) else {
            continue;
        };
        children.push(fragment);
    }
    children
}

fn build_svg_child_fragment(
    node: &SVGResolvedNode<'_>,
    layout_context: &LayoutContext,
    positioning_context: &mut PositioningContext,
    resource_graph: &SVGResourceGraph,
    nodes_by_opaque: &SVGNodeMap<'_>,
    identity_context: SVGFragmentIdentityContext,
) -> Option<Fragment> {
    let style_context = &layout_context.style_context;
    let resolved = resolved_node_resources(resource_graph, node.tag.node);
    let base_fragment_info = identity_context.base_fragment_info(node.tag);
    let identity = identity_context.fragment_identity(node.tag);

    match (&node.summary.kind, &node.svg_data.node_kind, &node.resolved_style) {
        (SVGLayoutNodeKind::Group, _, SVGNodeResolvedStyle::Geometry(style)) => {
            let children = build_svg_children(
                node,
                layout_context,
                positioning_context,
                resource_graph,
                nodes_by_opaque,
                identity_context,
            );
            let rect = union_fragment_rects(&children);
            Some(Fragment::SVGGroup(crate::cell::ArcRefCell::new(
                SVGGroupFragment {
                    base: BaseFragment::new(base_fragment_info, node.computed_style.clone().into(), rect),
                    identity,
                    children,
                    local_transform: parse_svg_transform(node.svg_data.common.transform),
                    opacity: style.opacity,
                    resources: resolved.resources.clone(),
                },
            )))
        }
        (SVGLayoutNodeKind::Use, _, SVGNodeResolvedStyle::Geometry(style)) => {
            let expansion = expand_use_node(node, nodes_by_opaque, resource_graph);
            let referenced_identity_context = identity_context.for_expanded_use(node.tag);
            let mut children = Vec::new();
            for referenced in expansion.referenced_node.into_iter() {
                let Some(referenced) = resolve_svg_node(referenced, style_context) else {
                    continue;
                };
                let Some(fragment) = build_svg_child_fragment(
                    &referenced,
                    layout_context,
                    positioning_context,
                    resource_graph,
                    nodes_by_opaque,
                    referenced_identity_context.clone(),
                ) else {
                    continue;
                };
                children.push(fragment);
            }
            let rect = union_fragment_rects(&children);
            Some(Fragment::SVGGroup(crate::cell::ArcRefCell::new(
                SVGGroupFragment {
                    base: BaseFragment::new(base_fragment_info, node.computed_style.clone().into(), rect),
                    identity,
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
            let children = layout_foreign_object_children(
                node,
                layout_context,
                positioning_context,
                viewport_rect,
            );
            Some(Fragment::SVGForeignObject(crate::cell::ArcRefCell::new(
                SVGForeignObjectFragment {
                    base: BaseFragment::new(
                        base_fragment_info,
                        node.computed_style.clone().into(),
                        physical_rect_from_svg_rect(viewport_rect),
                    ),
                    identity,
                    children,
                    svg_viewport_rect: viewport_rect,
                    local_transform: foreign_object.local_transform,
                },
            )))
        }
        (
            SVGLayoutNodeKind::Geometry,
            SVGNodeKind::Geometry(geometry),
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
                identity,
                path,
                object_bounding_box,
                decorated_bounding_box,
                local_transform: parse_svg_transform(node.svg_data.common.transform),
                fill: convert_resolved_paint(resource_graph, node.tag.node, &style.paint.fill),
                stroke: style.paint.stroke.as_ref().map(|stroke| {
                    convert_stroke_style(resource_graph, node.tag.node, stroke)
                }),
                resources: resolved.resources.clone(),
            })))
        }
        (SVGLayoutNodeKind::Text, _, SVGNodeResolvedStyle::Text(_)) => {
            let text_layout = layout_svg_text(node, layout_context);
            Some(Fragment::SVGText(crate::cell::ArcRefCell::new(SVGTextFragment {
                base: BaseFragment::new(
                    base_fragment_info,
                    node.computed_style.clone().into(),
                    physical_rect_from_svg_rect(text_layout.decorated_bounding_box),
                ),
                identity,
                text_runs: text_layout.text_runs,
                object_bounding_box: text_layout.object_bounding_box,
                decorated_bounding_box: text_layout.decorated_bounding_box,
                local_transform: parse_svg_transform(node.svg_data.common.transform),
                resources: resolved.resources.clone(),
            })))
        }
        (SVGLayoutNodeKind::Image, SVGNodeKind::Image(image), SVGNodeResolvedStyle::Geometry(_)) => {
            let viewport_rect = image_viewport(image);
            Some(Fragment::SVGImage(crate::cell::ArcRefCell::new(SVGImageFragment {
                base: BaseFragment::new(
                    base_fragment_info,
                    node.computed_style.clone().into(),
                    physical_rect_from_svg_rect(viewport_rect),
                ),
                identity,
                viewport_rect,
                local_transform: parse_svg_transform(node.svg_data.common.transform),
                href: image.href.map(str::to_owned),
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

fn build_svg_resource_graph<'dom>(
    root: &SVGResolvedNode<'dom>,
    style_context: &SharedStyleContext,
) -> (SVGResourceGraph, SVGNodeMap<'dom>) {
    let mut nodes = Vec::new();
    let mut nodes_by_opaque = FxHashMap::default();
    collect_resource_graph_node(root, style_context, None, &mut nodes, &mut nodes_by_opaque);
    (SVGResourceGraph::build(&nodes), nodes_by_opaque)
}

fn resolve_svg_resources(
    root: &SVGResolvedNode<'_>,
    style_context: &SharedStyleContext,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &mut SVGResourceGraph,
) {
    let mut visiting = Vec::new();
    resolve_svg_resource_node(root, style_context, nodes_by_opaque, resource_graph, &mut visiting);
}

fn resolve_svg_resource_node(
    node: &SVGResolvedNode<'_>,
    style_context: &SharedStyleContext,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &mut SVGResourceGraph,
    visiting: &mut Vec<OpaqueNode>,
) {
    match &node.svg_data.node_kind {
        SVGNodeKind::Gradient(_) => {
            if let Some(resource_id) = resource_graph.resource_id_for_node(node.tag.node) {
                if let Some(gradient) = resolve_gradient_resource(
                    node,
                    style_context,
                    nodes_by_opaque,
                    resource_graph,
                    visiting,
                ) {
                    if let Some(SVGResourceKind::Gradient(resource)) =
                        resource_graph.resource_mut(resource_id)
                    {
                        *resource = gradient;
                    }
                }
            }
        }
        SVGNodeKind::ClipPath(_) => {
            if let Some(resource_id) = resource_graph.resource_id_for_node(node.tag.node) {
                let clip_path =
                    resolve_clip_path_resource(node, style_context, nodes_by_opaque, resource_graph);
                if let Some(SVGResourceKind::ClipPath(resource)) =
                    resource_graph.resource_mut(resource_id)
                {
                    *resource = clip_path;
                }
            }
        }
        SVGNodeKind::Use(_) => {
            if let Some(use_resource_id) = resource_graph
                .node_resources(node.tag.node)
                .and_then(|resolved| resolved.use_instance_source)
            {
                let dependencies = resource_graph
                    .node_resources(node.tag.node)
                    .and_then(|resolved| resolved.referenced_node)
                    .and_then(|source| nodes_by_opaque.get(&source).copied())
                    .map(|source| {
                        collect_subtree_resource_dependencies(source, style_context, resource_graph)
                    })
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

    for child in node.node.children() {
        let Some(child) = resolve_svg_child_node(child, style_context, node) else {
            continue;
        };
        resolve_svg_resource_node(&child, style_context, nodes_by_opaque, resource_graph, visiting);
    }
}

fn resolve_gradient_resource(
    node: &SVGResolvedNode<'_>,
    style_context: &SharedStyleContext,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &SVGResourceGraph,
    visiting: &mut Vec<OpaqueNode>,
) -> Option<SVGGradientResource> {
    if visiting.contains(&node.tag.node) {
        return None;
    }
    visiting.push(node.tag.node);

    let template = gradient_template_node(node, style_context, nodes_by_opaque, resource_graph)
        .and_then(|template| {
            resolve_gradient_resource(&template, style_context, nodes_by_opaque, resource_graph, visiting)
        });

    let mut gradient = template.unwrap_or_else(default_gradient_resource);
    match &node.svg_data.node_kind {
        SVGNodeKind::Gradient(SVGGradientData::Linear {
            x1,
            y1,
            x2,
            y2,
            gradient_units,
            gradient_transform,
            spread_method,
            ..
        }) => {
            gradient.units = parse_coordinate_units(*gradient_units).unwrap_or(gradient.units);
            gradient.gradient_transform = parse_svg_transform(*gradient_transform);
            gradient.spread_method = parse_spread_method(*spread_method)
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
                    parse_gradient_length(*x1, linear.start.x),
                    parse_gradient_length(*y1, linear.start.y),
                ),
                end: SVGPoint::new(
                    parse_gradient_length(*x2, linear.end.x),
                    parse_gradient_length(*y2, linear.end.y),
                ),
            });
        }
        SVGNodeKind::Gradient(SVGGradientData::Radial {
            cx,
            cy,
            r,
            fx,
            fy,
            fr,
            gradient_units,
            gradient_transform,
            spread_method,
            ..
        }) => {
            gradient.units = parse_coordinate_units(*gradient_units).unwrap_or(gradient.units);
            gradient.gradient_transform = parse_svg_transform(*gradient_transform);
            gradient.spread_method = parse_spread_method(*spread_method)
                .unwrap_or(gradient.spread_method);
            let radial = match gradient.kind {
                SVGGradientKind::Radial(ref radial) => radial.clone(),
                _ => SVGRadialGradient {
                    center: SVGPoint::new(0.5, 0.5),
                    focal: SVGPoint::new(0.5, 0.5),
                    radius: 0.5,
                    focal_radius: 0.0,
                },
            };
            let center = SVGPoint::new(
                parse_gradient_length(*cx, radial.center.x),
                parse_gradient_length(*cy, radial.center.y),
            );
            gradient.kind = SVGGradientKind::Radial(SVGRadialGradient {
                center,
                focal: SVGPoint::new(
                    parse_gradient_length(*fx, center.x),
                    parse_gradient_length(*fy, center.y),
                ),
                radius: parse_gradient_length(*r, radial.radius),
                focal_radius: parse_gradient_length(*fr, radial.focal_radius),
            });
        }
        _ => {}
    }

    let stops = collect_gradient_stops(node, style_context);
    if !stops.is_empty() {
        gradient.stops = stops;
    }

    visiting.pop();
    gradient.gradient_transform.is_invertible().then_some(gradient)
}

fn resolve_clip_path_resource(
    node: &SVGResolvedNode<'_>,
    style_context: &SharedStyleContext,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &SVGResourceGraph,
) -> SVGClipPathResource {
    let (units, transform) = match &node.svg_data.node_kind {
        SVGNodeKind::ClipPath(data) => (
            parse_coordinate_units(data.clip_path_units)
                .unwrap_or(SVGCoordinateUnits::UserSpaceOnUse),
            parse_svg_transform(node.svg_data.common.transform),
        ),
        _ => (SVGCoordinateUnits::UserSpaceOnUse, SVGTransform::identity()),
    };
    SVGClipPathResource {
        units,
        transform,
        paths: collect_clip_paths(
            node,
            style_context,
            nodes_by_opaque,
            resource_graph,
            SVGTransform::identity(),
        ),
    }
}

fn collect_clip_paths(
    node: &SVGResolvedNode<'_>,
    style_context: &SharedStyleContext,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &SVGResourceGraph,
    inherited_transform: SVGTransform,
) -> Vec<SVGPathData> {
    let node_transform = parse_svg_transform(node.svg_data.common.transform);
    let combined_transform = then_svg_transform(inherited_transform, node_transform);
    match (&node.summary.kind, &node.svg_data.node_kind, &node.resolved_style) {
        (
            SVGLayoutNodeKind::Geometry,
            SVGNodeKind::Geometry(geometry),
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
            .node
            .children()
            .filter_map(|child| {
                let child = resolve_svg_child_node(child, style_context, node)?;
                Some(collect_clip_paths(
                    &child,
                    style_context,
                    nodes_by_opaque,
                    resource_graph,
                    combined_transform,
                ))
            })
            .flatten()
            .collect(),
        (SVGLayoutNodeKind::Use, _, _) => {
            let expansion = expand_use_node(node, nodes_by_opaque, resource_graph);
            let use_transform = then_svg_transform(combined_transform, expansion.instance_transform);
            expansion
                .referenced_node
                .into_iter()
                .filter_map(|referenced| {
                    let referenced = resolve_svg_node(referenced, style_context)?;
                    Some(collect_clip_paths(
                        &referenced,
                        style_context,
                        nodes_by_opaque,
                        resource_graph,
                        use_transform,
                    ))
                })
                .flatten()
                .collect()
        }
        _ => Vec::new(),
    }
}

fn collect_gradient_stops(
    node: &SVGResolvedNode<'_>,
    style_context: &SharedStyleContext,
) -> Vec<SVGGradientStop> {
    node.node
        .children()
        .filter_map(|child| {
            let child = resolve_svg_child_node(child, style_context, node)?;
            match &child.svg_data.node_kind {
                SVGNodeKind::Stop(stop) => Some(resolve_gradient_stop(stop, &child)),
                _ => None,
            }
        })
        .collect()
}

fn resolve_gradient_stop(
    stop: &layout_api::SVGStopData<'_>,
    node: &SVGResolvedNode<'_>,
) -> SVGGradientStop {
    let color = match &node.resolved_style {
        SVGNodeResolvedStyle::Geometry(style) => style.paint.current_color,
        SVGNodeResolvedStyle::Viewport { geometry, .. } => geometry.paint.current_color,
        SVGNodeResolvedStyle::Text(text) => text.paint.current_color,
    };
    SVGGradientStop {
        offset: parse_stop_offset(stop.offset).unwrap_or(0.0),
        color: stop
            .stop_color
            .map(str::to_owned)
            .or_else(|| inline_style_property(node, "stop-color"))
            .as_deref()
            .and_then(parse_svg_color)
            .unwrap_or(color),
        opacity: stop
            .stop_opacity
            .map(str::to_owned)
            .or_else(|| inline_style_property(node, "stop-opacity"))
            .as_deref()
            .and_then(parse_unit_interval)
            .unwrap_or(1.0),
    }
}

fn inline_style_property(node: &SVGResolvedNode<'_>, property: &str) -> Option<String> {
    let element = node.node.as_element()?;
    let style = element.get_attr(&ns!(), &local_name!("style"))?;
    style.rsplit(';').find_map(|declaration| {
        let (name, value) = declaration.split_once(':')?;
        (name.trim().eq_ignore_ascii_case(property)).then_some(value.trim().to_owned())
    })
}

fn gradient_template_node<'dom>(
    node: &SVGResolvedNode<'dom>,
    style_context: &SharedStyleContext,
    nodes_by_opaque: &SVGNodeMap<'dom>,
    resource_graph: &SVGResourceGraph,
) -> Option<SVGResolvedNode<'dom>> {
    let href = match &node.svg_data.node_kind {
        SVGNodeKind::Gradient(SVGGradientData::Linear { href, .. })
        | SVGNodeKind::Gradient(SVGGradientData::Radial { href, .. }) => *href,
        _ => None,
    }?;
    let id = href.trim().strip_prefix('#')?;
    let target = resource_graph.node_for_element_id(id)?;
    let target = nodes_by_opaque.get(&target).copied()?;
    let target = resolve_svg_node(target, style_context)?;
    matches!(target.svg_data.node_kind, SVGNodeKind::Gradient(_)).then_some(target)
}

fn collect_subtree_resource_dependencies(
    node: ServoThreadSafeLayoutNode<'_>,
    style_context: &SharedStyleContext,
    resource_graph: &SVGResourceGraph,
) -> Vec<SVGResourceId> {
    let mut resources = Vec::new();
    collect_subtree_resource_dependencies_into(node, style_context, resource_graph, &mut resources);
    resources.sort_by_key(|id| id.0);
    resources.dedup();
    resources
}

fn collect_subtree_resource_dependencies_into(
    node: ServoThreadSafeLayoutNode<'_>,
    style_context: &SharedStyleContext,
    resource_graph: &SVGResourceGraph,
    resources: &mut Vec<SVGResourceId>,
) {
    let Some(node) = resolve_svg_node(node, style_context) else {
        return;
    };
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
    for child in node.node.children() {
        collect_subtree_resource_dependencies_into(child, style_context, resource_graph, resources);
    }
}

fn collect_resource_graph_node<'dom>(
    node: &SVGResolvedNode<'dom>,
    style_context: &SharedStyleContext,
    parent: Option<style::dom::OpaqueNode>,
    nodes: &mut Vec<SVGResourceGraphNode>,
    nodes_by_opaque: &mut SVGNodeMap<'dom>,
) {
    let mut graph_node = SVGResourceGraphNode::new(node.tag.node, node.summary.kind);
    nodes_by_opaque.insert(node.tag.node, node.node);
    graph_node.parent = parent;
    graph_node.element_id = node.svg_data.common.element_id.map(str::to_owned);
    graph_node.establishes_viewport = node.summary.establishes_viewport;
    graph_node.participates_in_paint = node.summary.participates_in_paint;

    match (&node.svg_data.node_kind, &node.resolved_style) {
        (SVGNodeKind::Viewport(_), SVGNodeResolvedStyle::Viewport { geometry, .. }) => {
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

    graph_node.href = match &node.svg_data.node_kind {
        SVGNodeKind::Use(data) => data.href.map(str::to_owned),
        SVGNodeKind::Gradient(SVGGradientData::Linear { href, .. })
        | SVGNodeKind::Gradient(SVGGradientData::Radial { href, .. }) => href.map(str::to_owned),
        _ => None,
    };

    nodes.push(graph_node);
    for child in node.node.children() {
        let Some(child) = resolve_svg_child_node(child, style_context, node) else {
            continue;
        };
        collect_resource_graph_node(
            &child,
            style_context,
            Some(node.tag.node),
            nodes,
            nodes_by_opaque,
        );
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
            let iri = reference.iri.trim();
            let id = iri.strip_prefix('#').unwrap_or(iri);
            let resource_id = resource_graph
                .resource_for_element_id(id)
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

fn image_viewport(image: &layout_api::SVGImageData<'_>) -> SVGRect {
    SVGRect::new(
        euclid::point2(
            parse_svg_length(image.x).unwrap_or(0.0),
            parse_svg_length(image.y).unwrap_or(0.0),
        ),
        euclid::size2(
            parse_svg_length(image.width).unwrap_or(0.0),
            parse_svg_length(image.height).unwrap_or(0.0),
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

#[cfg(test)]
mod tests {
    use super::*;
    use havi_types::fragment_tree::{SVGGradientResource, SVGLinearGradient, SVGResourceNode};
    use layout_api::wrapper_traits::PseudoElementChain;

    use crate::svg::style;

    fn test_tag(id: usize) -> Tag {
        Tag {
            node: OpaqueNode(id),
            pseudo_element_chain: PseudoElementChain::default(),
        }
    }

    #[test]
    fn convert_resolved_paint_accepts_func_iri_without_hash_prefix() {
        let mut resource_graph = SVGResourceGraph::default();
        resource_graph.resources.push(SVGResourceNode {
            kind: SVGResourceKind::Gradient(SVGGradientResource {
                units: SVGCoordinateUnits::ObjectBoundingBox,
                gradient_transform: SVGTransform::identity(),
                spread_method: SVGGradientSpreadMethod::Pad,
                kind: SVGGradientKind::Linear(SVGLinearGradient {
                    start: SVGPoint::new(0.0, 0.0),
                    end: SVGPoint::new(1.0, 0.0),
                }),
                stops: Vec::new(),
            }),
        });
        resource_graph
            .resources_by_element_id
            .insert("grad".to_owned(), SVGResourceId(0));

        let paint = convert_resolved_paint(
            &resource_graph,
            OpaqueNode(0),
            &SVGResolvedPaint::ResourceReference(style::SVGPaintServerReference {
                iri: "grad".to_owned(),
                fallback: Some(SVGPaintFallback::SolidColor(SVGColor {
                    red: 0.0,
                    green: 1.0,
                    blue: 0.0,
                    alpha: 1.0,
                })),
            }),
        );

        match paint {
            SVGPaint::Resource(SVGResourceId(0)) => {}
            other => panic!("expected gradient resource, got {other:?}"),
        }
    }

    #[test]
    fn nested_use_identity_keeps_immediate_owner_and_parent_chain() {
        let outer_use = test_tag(1);
        let inner_use = test_tag(2);
        let source_descendant = test_tag(3);

        let identity = SVGFragmentIdentityContext::default()
            .for_expanded_use(outer_use)
            .for_expanded_use(inner_use)
            .fragment_identity(source_descendant);

        assert_eq!(identity.source_tag.node, source_descendant.node);
        assert_eq!(
            identity.current_instance_owner_tag().map(|tag| tag.node),
            Some(inner_use.node),
        );

        let parent = identity
            .instance_chain
            .as_ref()
            .and_then(|chain| chain.parent.as_ref())
            .expect("nested use should preserve parent chain");
        assert_eq!(parent.owner_tag.node, outer_use.node);
    }
}
