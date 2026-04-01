use std::str::FromStr;

use app_units::Au;
use malloc_size_of_derive::MallocSizeOf;
use rustc_hash::FxHashMap;
use servo_arc::Arc as ServoArc;
use style::computed_values::object_fit::T as ObjectFit;
use style::Zero;
use style::dom::{OpaqueNode, TNode};
use style::logical_geometry::{Direction, WritingMode};
use style::properties::ComputedValues;
use style::values::CSSFloat;

use havi_types::fragment_tree::{
    SVGBounds, SVGClipPathResource, SVGColor, SVGContainerKind, SVGCoordinateUnits,
    SVGEffectState, SVGFragmentIdentity, SVGGradientKind, SVGGradientResource,
    SVGGradientSpreadMethod, SVGGradientStop, SVGImagePayload, SVGLeafKind, SVGLength,
    SVGLengthUnit, SVGLinearGradient, SVGMeetOrSlice, SVGPaint, SVGPaintServerResource,
    SVGPaintStyle, SVGPathData, SVGPathPayload, SVGPatternRect, SVGPoint,
    SVGPreserveAspectRatio, SVGPreserveAspectRatioAlign, SVGRect, SVGResourceId,
    SVGResourceKind, SVGStrokeStyle, SVGTransform, SVGUseInstanceChain,
    SVGRadialGradient, Tag as PublishedTag,
};
use layout_api::wrapper_traits::ThreadSafeLayoutNode;
use layout_api::{SVGElementData, SVGGradientData, SVGNodeKind};
use script::layout_dom::{ServoLayoutNode, ServoThreadSafeLayoutNode};

use super::dom::{build_dom_svg_tree, SVGLayoutNodeKind, SVGNodeResolvedStyle};
use super::foreign_object::{layout_foreign_object, layout_foreign_object_children};
use super::path::{
    decorated_bounds, normalize_svg_geometry, path_bounds, resolve_length, transform_svg_path_data,
};
use super::resources::{
    SVGResolvedNodeResources, SVGResourceGraph, SVGResourceGraphNode, SVGResourceReferenceInputs,
};
use super::style::{SVGPaintFallback, SVGResolvedPaint};
use super::text::{SVGTextLayoutContext, layout_svg_text};
use super::transform::{compute_view_box_mapper, parse_svg_transform, then_svg_transform};
use super::tree::{
    SVGResolvedNode, SVGResolvedNodeMap, SVGStandaloneTree, build_resolved_node_map,
};
use super::use_expansion::expand_use_node;
use crate::context::LayoutContext;
use crate::dom::NodeExt;
use crate::fragment_tree::{
    BaseFragment, BaseFragmentInfo, CollapsedBlockMargins, Fragment, FragmentFlags,
    SVGContainerFragment, SVGLeafFragment, SVGResourceOwnedSubtree, SVGViewportFragment, Tag,
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
    standalone_viewbox_fill: bool,
}

impl SVGRootContents {
    pub(crate) fn for_element(
        node: ServoThreadSafeLayoutNode<'_>,
        _context: &LayoutContext,
    ) -> Option<Self> {
        let svg_data = node.as_svg().filter(|data| data.viewport().is_some())?;
        let standalone_document_root = node.unsafe_get().traversal_parent().is_none();
        Some(Self {
            root_node_id: node.unsafe_get().trusted_node_id(),
            intrinsic_size: intrinsic_svg_root_sizes(&svg_data),
            standalone_viewbox_fill: standalone_document_root && svg_data.viewport().is_some_and(|viewport| {
                viewport.width.is_none() && viewport.height.is_none() && viewport.view_box.is_some()
            }),
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
        let rect = if self.standalone_viewbox_fill {
            let viewport = layout_context.style_context.stylist.device().au_viewport_size();
            PhysicalRect::new(
                PhysicalPoint::new(Au::zero(), Au::zero()),
                PhysicalSize::new(viewport.width, viewport.height),
            )
        } else {
            self.calculate_fragment_rect(&base.style, size)
        };
        let root_node = ServoThreadSafeLayoutNode::new(ServoLayoutNode::from_trusted_node_id(
            self.root_node_id,
        ));
        let fragments = build_svg_root_fragment(
            root_node,
            self.standalone_viewbox_fill,
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
    let width = resolve_length(viewport.width)
        .filter(|width| *width >= 0.0)
        .or_else(|| viewport.view_box.filter(|view_box| view_box.width > 0.0).map(|view_box| view_box.width));
    let height = resolve_length(viewport.height)
        .filter(|height| *height >= 0.0)
        .or_else(|| viewport.view_box.filter(|view_box| view_box.height > 0.0).map(|view_box| view_box.height));

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
    standalone_viewbox_fill: bool,
    layout_context: &LayoutContext,
    positioning_context: &mut PositioningContext,
    base_fragment_info: BaseFragmentInfo,
    outer_style: &ServoArc<ComputedValues>,
    outer_rect: PhysicalRect<Au>,
) -> Option<Fragment> {
    let adapter = build_dom_svg_tree(root, &layout_context.style_context)?;
    let text_layout_context = SVGTextLayoutContext::from(layout_context);
    build_svg_root_fragment_from_resolved_tree(
        &adapter.resolve(),
        Some(&adapter.dom_nodes),
        standalone_viewbox_fill,
        false,
        &text_layout_context,
        Some(layout_context),
        positioning_context,
        base_fragment_info,
        outer_style.clone(),
        outer_rect,
    )
}

pub(crate) fn build_svg_root_fragment_from_tree(
    tree: &SVGStandaloneTree,
    default_style: ServoArc<ComputedValues>,
    layout_context: &LayoutContext,
    positioning_context: &mut PositioningContext,
    base_fragment_info: BaseFragmentInfo,
    outer_style: &ServoArc<ComputedValues>,
    outer_rect: PhysicalRect<Au>,
) -> Option<Fragment> {
    let text_layout_context = SVGTextLayoutContext::from(layout_context);
    build_svg_root_fragment_from_tree_with_text_context(
        tree,
        default_style,
        &text_layout_context,
        positioning_context,
        base_fragment_info,
        outer_style,
        outer_rect,
    )
}

pub(crate) fn build_svg_root_fragment_from_tree_with_text_context(
    tree: &SVGStandaloneTree,
    default_style: ServoArc<ComputedValues>,
    text_layout_context: &SVGTextLayoutContext,
    positioning_context: &mut PositioningContext,
    base_fragment_info: BaseFragmentInfo,
    outer_style: &ServoArc<ComputedValues>,
    outer_rect: PhysicalRect<Au>,
) -> Option<Fragment> {
    let resolved = tree.resolve_with_default_style(default_style);
    build_svg_root_fragment_from_resolved_tree(
        &resolved,
        None,
        false,
        true,
        text_layout_context,
        None,
        positioning_context,
        base_fragment_info,
        outer_style.clone(),
        outer_rect,
    )
}

fn build_svg_root_fragment_from_resolved_tree(
    root: &SVGResolvedNode,
    dom_nodes_by_id: Option<&FxHashMap<super::tree::SVGNodeId, ServoThreadSafeLayoutNode<'_>>>,
    standalone_viewbox_fill: bool,
    force_root_overflow_clip: bool,
    text_layout_context: &SVGTextLayoutContext,
    dom_layout_context: Option<&LayoutContext>,
    positioning_context: &mut PositioningContext,
    base_fragment_info: BaseFragmentInfo,
    outer_style: ServoArc<ComputedValues>,
    outer_rect: PhysicalRect<Au>,
) -> Option<Fragment> {
    if root.summary.kind != SVGLayoutNodeKind::Viewport {
        return None;
    }
    let nodes_by_opaque = build_resolved_node_map(root);
    let mut resource_graph = build_svg_resource_graph(root);
    resolve_svg_resources(root, &nodes_by_opaque, &mut resource_graph);
    build_svg_node_fragment(
        root,
        dom_nodes_by_id,
        standalone_viewbox_fill,
        force_root_overflow_clip,
        text_layout_context,
        dom_layout_context,
        positioning_context,
        base_fragment_info,
        outer_style,
        outer_rect,
        resource_graph,
        &nodes_by_opaque,
    )
}

type SVGNodeMap<'a> = SVGResolvedNodeMap<'a>;

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
    node: &SVGResolvedNode,
    dom_nodes_by_id: Option<&FxHashMap<super::tree::SVGNodeId, ServoThreadSafeLayoutNode<'_>>>,
    standalone_viewbox_fill: bool,
    force_root_overflow_clip: bool,
    text_layout_context: &SVGTextLayoutContext,
    dom_layout_context: Option<&LayoutContext>,
    positioning_context: &mut PositioningContext,
    base_fragment_info: BaseFragmentInfo,
    style: ServoArc<ComputedValues>,
    rect: PhysicalRect<Au>,
    resource_graph: SVGResourceGraph,
    nodes_by_opaque: &SVGNodeMap<'_>,
) -> Option<Fragment> {
    match (&node.summary.kind, &node.svg_data().node_kind, &node.resolved_style) {
        (
            SVGLayoutNodeKind::Viewport,
            SVGNodeKind::Viewport(viewport),
            SVGNodeResolvedStyle::Viewport { viewport: viewport_style, .. },
        ) => {
            let viewport_rect = svg_rect_from_physical_rect(rect);
            let view_box_rect = viewport.view_box.map(svg_rect_from_view_box);
            let preserve_aspect_ratio = if standalone_viewbox_fill && viewport.width.is_none() && viewport.height.is_none() {
                layout_api::SVGPreserveAspectRatioValue {
                    align: layout_api::SVG_PRESERVEASPECTRATIO_NONE,
                    meet_or_slice: layout_api::SVG_MEETORSLICE_MEET,
                }
            } else {
                viewport_style.preserve_aspect_ratio
            };
            let mapper = compute_view_box_mapper(
                viewport_rect,
                view_box_rect,
                preserve_aspect_ratio,
            );
            let viewport_transform = then_svg_transform(
                mapper.local_to_parent,
                parse_svg_transform(&node.svg_data().common.transform),
            );
            let overflow_clip = (force_root_overflow_clip || viewport_style.overflow_hidden).then_some(
                havi_types::fragment_tree::SVGOverflowClip {
                    enabled: true,
                    rect: viewport_rect,
                },
            );
            let children = build_svg_children(
                node,
                dom_nodes_by_id,
                text_layout_context,
                dom_layout_context,
                positioning_context,
                &resource_graph,
                nodes_by_opaque,
                SVGFragmentIdentityContext::default(),
            );
            let resource_owned_subtrees = build_svg_resource_owned_subtrees(
                node,
                dom_nodes_by_id,
                text_layout_context,
                dom_layout_context,
                positioning_context,
                &resource_graph,
                nodes_by_opaque,
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
                    resource_owned_subtrees,
                },
            )))
        }
        _ => None,
    }
}

fn build_svg_children(
    node: &SVGResolvedNode,
    dom_nodes_by_id: Option<&FxHashMap<super::tree::SVGNodeId, ServoThreadSafeLayoutNode<'_>>>,
    text_layout_context: &SVGTextLayoutContext,
    dom_layout_context: Option<&LayoutContext>,
    positioning_context: &mut PositioningContext,
    resource_graph: &SVGResourceGraph,
    nodes_by_opaque: &SVGNodeMap<'_>,
    identity_context: SVGFragmentIdentityContext,
) -> Vec<Fragment> {
    let mut children = Vec::new();
    for child in node.element_children() {
        let Some(fragment) = build_svg_child_fragment(
            child,
            dom_nodes_by_id,
            text_layout_context,
            dom_layout_context,
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

fn build_svg_resource_owned_subtrees(
    _viewport: &SVGResolvedNode,
    dom_nodes_by_id: Option<&FxHashMap<super::tree::SVGNodeId, ServoThreadSafeLayoutNode<'_>>>,
    text_layout_context: &SVGTextLayoutContext,
    dom_layout_context: Option<&LayoutContext>,
    positioning_context: &mut PositioningContext,
    resource_graph: &SVGResourceGraph,
    nodes_by_opaque: &SVGNodeMap<'_>,
) -> Vec<SVGResourceOwnedSubtree> {
    resource_graph
        .resources()
        .iter()
        .enumerate()
        .filter_map(|(index, resource)| {
            let resource_id = SVGResourceId(index as u32);
            let owner = resource_graph.resource_owner(resource_id)?;
            let owner = nodes_by_opaque.get(&owner).copied()?;
            match &resource.kind {
                SVGResourceKind::PaintServer(SVGPaintServerResource::Pattern(_)) => {
                    let resolved = resolve_pattern_resource_data(
                        owner,
                        nodes_by_opaque,
                        resource_graph,
                        &mut Vec::new(),
                    )?;
                    let content_source = resolved.content_source_node?;
                    let mut fragment_roots = Vec::new();
                    for child in content_source.element_children() {
                        let Some(fragment) = build_svg_child_fragment(
                            child,
                            dom_nodes_by_id,
                            text_layout_context,
                            dom_layout_context,
                            positioning_context,
                            resource_graph,
                            nodes_by_opaque,
                            SVGFragmentIdentityContext::default(),
                        ) else {
                            continue;
                        };
                        fragment_roots.push(fragment);
                    }
                    if fragment_roots.is_empty() {
                        return None;
                    }
                    let mut resource_dependencies = resolved.source_resource_dependencies;
                    resource_dependencies.sort_by_key(|id| id.0);
                    resource_dependencies.dedup();
                    Some(SVGResourceOwnedSubtree {
                        owner_resource_id: resource_id,
                        fragment_roots,
                        resource_dependencies,
                    })
                }
                SVGResourceKind::Marker(_) => None,
                _ => None,
            }
        })
        .collect()
}

fn build_svg_child_fragment(
    node: &SVGResolvedNode,
    dom_nodes_by_id: Option<&FxHashMap<super::tree::SVGNodeId, ServoThreadSafeLayoutNode<'_>>>,
    text_layout_context: &SVGTextLayoutContext,
    dom_layout_context: Option<&LayoutContext>,
    positioning_context: &mut PositioningContext,
    resource_graph: &SVGResourceGraph,
    nodes_by_opaque: &SVGNodeMap<'_>,
    identity_context: SVGFragmentIdentityContext,
) -> Option<Fragment> {
    let resolved = resolved_node_resources(resource_graph, node.tag.node);
    let base_fragment_info = identity_context.base_fragment_info(node.tag);
    let identity = identity_context.fragment_identity(node.tag);

    match (&node.summary.kind, &node.svg_data().node_kind, &node.resolved_style) {
        (SVGLayoutNodeKind::Group, _, SVGNodeResolvedStyle::Geometry(_style)) => {
            let children = build_svg_children(
                node,
                dom_nodes_by_id,
                text_layout_context,
                dom_layout_context,
                positioning_context,
                resource_graph,
                nodes_by_opaque,
                identity_context,
            );
            let rect = union_fragment_rects(&children);
            Some(Fragment::SVGContainer(crate::cell::ArcRefCell::new(
                SVGContainerFragment {
                    base: BaseFragment::new(base_fragment_info, node.computed_style.clone().into(), rect),
                    identity,
                    kind: SVGContainerKind::Group,
                    children,
                    local_transform: parse_svg_transform(&node.svg_data().common.transform),
                    effects: convert_effect_state(resolved.resources.clone()),
                },
            )))
        }
        (SVGLayoutNodeKind::Use, _, SVGNodeResolvedStyle::Geometry(_style)) => {
            let expansion = expand_use_node(node, nodes_by_opaque, resource_graph);
            let referenced_identity_context = identity_context.for_expanded_use(node.tag);
            let mut children = Vec::new();
            for referenced in expansion.referenced_node.into_iter() {
                let Some(fragment) = build_svg_child_fragment(
                    referenced,
                    dom_nodes_by_id,
                    text_layout_context,
                    dom_layout_context,
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
            Some(Fragment::SVGContainer(crate::cell::ArcRefCell::new(
                SVGContainerFragment {
                    base: BaseFragment::new(base_fragment_info, node.computed_style.clone().into(), rect),
                    identity,
                    kind: SVGContainerKind::Group,
                    children,
                    local_transform: expansion.instance_transform,
                    effects: convert_effect_state(resolved.resources.clone()),
                },
            )))
        }
        (SVGLayoutNodeKind::ForeignObject, _, SVGNodeResolvedStyle::Geometry(_)) => {
            let foreign_object = layout_foreign_object(node);
            let viewport_rect = foreign_object.viewport_rect.unwrap_or_default();
            let children = dom_nodes_by_id
                .and_then(|dom_nodes| dom_nodes.get(&node.id).copied())
                .and_then(|dom_node| {
                    dom_layout_context.map(|layout_context| {
                        layout_foreign_object_children(
                            dom_node,
                            node.computed_style.clone(),
                            layout_context,
                            positioning_context,
                            viewport_rect,
                        )
                    })
                })
                .unwrap_or_default();
            Some(Fragment::SVGContainer(crate::cell::ArcRefCell::new(
                SVGContainerFragment {
                    base: BaseFragment::new(
                        base_fragment_info,
                        node.computed_style.clone().into(),
                        physical_rect_from_svg_rect(viewport_rect),
                    ),
                    identity,
                    kind: SVGContainerKind::ForeignObject {
                        svg_viewport_rect: viewport_rect,
                    },
                    children,
                    local_transform: foreign_object.local_transform,
                    effects: convert_effect_state(resolved.resources.clone()),
                },
            )))
        }
        (
            SVGLayoutNodeKind::Geometry,
            SVGNodeKind::Geometry(geometry),
            SVGNodeResolvedStyle::Geometry(style),
        ) => {
            let path: SVGPathData = normalize_svg_geometry(geometry, style.fill_rule).into();
            let transformed_path = transform_svg_path_data(
                &path,
                parse_svg_transform(&node.svg_data().common.transform),
            );
            let object_bounding_box = path_bounds(&transformed_path).unwrap_or_default();
            let stroke_bounding_box = decorated_bounds(&transformed_path, style.paint.stroke.as_ref())
                .unwrap_or(object_bounding_box);
            let bounds = SVGBounds {
                object_bounding_box,
                stroke_bounding_box,
                decorated_bounding_box: stroke_bounding_box,
                visual_bounding_box: stroke_bounding_box,
            };
            Some(Fragment::SVGLeaf(crate::cell::ArcRefCell::new(SVGLeafFragment {
                base: BaseFragment::new(
                    base_fragment_info,
                    node.computed_style.clone().into(),
                    physical_rect_from_svg_rect(bounds.visual_bounding_box),
                ),
                identity,
                kind: SVGLeafKind::Path(SVGPathPayload {
                    path: transformed_path,
                }),
                bounds,
                local_transform: SVGTransform::identity(),
                paint: convert_paint_style(resource_graph, node.tag.node, &style.paint, style.opacity),
                effects: convert_effect_state(resolved.resources.clone()),
            })))
        }
        (SVGLayoutNodeKind::Text, _, SVGNodeResolvedStyle::Text(style)) => {
            let text_layout = layout_svg_text(node, text_layout_context, resource_graph, nodes_by_opaque);
            Some(Fragment::SVGLeaf(crate::cell::ArcRefCell::new(SVGLeafFragment { 
                base: BaseFragment::new(
                    base_fragment_info,
                    node.computed_style.clone().into(),
                    physical_rect_from_svg_rect(text_layout.bounds.visual_bounding_box),
                ),
                identity,
                kind: SVGLeafKind::Text(text_layout.payload),
                bounds: text_layout.bounds,
                local_transform: parse_svg_transform(&node.svg_data().common.transform),
                paint: convert_paint_style(resource_graph, node.tag.node, &style.paint, style.opacity),
                effects: convert_effect_state(resolved.resources.clone()),
            })))
        }
        (SVGLayoutNodeKind::Image, SVGNodeKind::Image(image), SVGNodeResolvedStyle::Geometry(style)) => {
            let viewport_rect = image_viewport(image);
            let bounds = image_bounds(viewport_rect, style.paint.stroke.as_ref());
            Some(Fragment::SVGLeaf(crate::cell::ArcRefCell::new(SVGLeafFragment {
                base: BaseFragment::new(
                    base_fragment_info,
                    node.computed_style.clone().into(),
                    physical_rect_from_svg_rect(bounds.visual_bounding_box),
                ),
                identity,
                kind: SVGLeafKind::Image(SVGImagePayload {
                    viewport_rect,
                    href: image.href.map(|reference| reference.raw.to_owned()),
                }),
                bounds,
                local_transform: parse_svg_transform(&node.svg_data().common.transform),
                paint: convert_paint_style(resource_graph, node.tag.node, &style.paint, style.opacity),
                effects: convert_effect_state(resolved.resources.clone()),
            })))
        }
        (SVGLayoutNodeKind::Defs, _, _)
        | (SVGLayoutNodeKind::Gradient, _, _)
        | (SVGLayoutNodeKind::Stop, _, _)
        | (SVGLayoutNodeKind::ClipPath, _, _)
        | (SVGLayoutNodeKind::Mask, _, _)
        // Pattern, Filter, Marker remain resource-owned nodes rather than visible SVG fragments.
        | (SVGLayoutNodeKind::Pattern, _, _)
        | (SVGLayoutNodeKind::Filter, _, _)
        | (SVGLayoutNodeKind::Marker, _, _)
        // TextPath: rendered as a child of <text> via layout_svg_text, not as a top-level fragment
        | (SVGLayoutNodeKind::TextPath, _, _) => None,
        _ => None,
    }
}

fn build_svg_resource_graph(root: &SVGResolvedNode) -> SVGResourceGraph {
    let mut nodes = Vec::new();
    collect_resource_graph_node(root, None, &mut nodes);
    SVGResourceGraph::build(&nodes)
}

fn resolve_svg_resources(
    root: &SVGResolvedNode,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &mut SVGResourceGraph,
) {
    let mut visiting = Vec::new();
    resolve_svg_resource_node(root, nodes_by_opaque, resource_graph, &mut visiting);
}

fn resolve_svg_resource_node(
    node: &SVGResolvedNode,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &mut SVGResourceGraph,
    visiting: &mut Vec<OpaqueNode>,
) {
    match &node.svg_data().node_kind {
        SVGNodeKind::Gradient(_) => {
            if let Some(resource_id) = resource_graph.resource_id_for_node(node.tag.node) {
                if let Some(gradient) = resolve_gradient_resource(
                    node,
                    nodes_by_opaque,
                    resource_graph,
                    visiting,
                ) {
                    if let Some(SVGResourceKind::PaintServer(SVGPaintServerResource::Gradient(resource))) =
                        resource_graph.resource_mut(resource_id)
                    {
                        *resource = gradient;
                    }
                } else {
                    unpublish_resource_reference(node, resource_graph);
                }
            }
        }
        SVGNodeKind::ClipPath(_) => {
            if let Some(resource_id) = resource_graph.resource_id_for_node(node.tag.node) {
                let clip_path =
                    resolve_clip_path_resource(node, nodes_by_opaque, resource_graph);
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
                        collect_subtree_resource_dependencies(source, nodes_by_opaque, resource_graph)
                    })
                    .unwrap_or_default();
                if let Some(SVGResourceKind::UseInstanceSource(resource)) =
                    resource_graph.resource_mut(use_resource_id)
                {
                    resource.source_resource_dependencies = dependencies;
                }
            }
        }
        SVGNodeKind::Pattern(_) => {
            if let Some(resource_id) = resource_graph.resource_id_for_node(node.tag.node) {
                if let Some(pattern) = resolve_pattern_resource_data(
                    node,
                    nodes_by_opaque,
                    resource_graph,
                    visiting,
                ) {
                    if let Some(SVGResourceKind::PaintServer(SVGPaintServerResource::Pattern(resource))) =
                        resource_graph.resource_mut(resource_id)
                    {
                        resource.units = pattern.units;
                        resource.content_units = pattern.content_units;
                        resource.pattern_transform = pattern.pattern_transform;
                        resource.rect = pattern.rect;
                        resource.view_box = pattern.view_box;
                        resource.preserve_aspect_ratio = pattern.preserve_aspect_ratio;
                        resource.source_fragment_roots.clear();
                        resource.source_resource_dependencies = pattern.source_resource_dependencies;
                    }
                } else {
                    unpublish_resource_reference(node, resource_graph);
                }
            }
        }
        SVGNodeKind::Mask(data) => {
            if let Some(resource_id) = resource_graph.resource_id_for_node(node.tag.node) {
                let units = data.mask_units.unwrap_or(SVGCoordinateUnits::ObjectBoundingBox);
                let content_units = data
                    .mask_content_units
                    .unwrap_or(SVGCoordinateUnits::UserSpaceOnUse);
                let rect = svg_resource_rect(
                    data.x,
                    data.y,
                    data.width,
                    data.height,
                    -0.1,
                    -0.1,
                    1.2,
                    1.2,
                );
                let paths = collect_mask_paths(
                    node,
                    nodes_by_opaque,
                    resource_graph,
                    SVGTransform::identity(),
                );
                if let Some(SVGResourceKind::Mask(resource)) =
                    resource_graph.resource_mut(resource_id)
                {
                    resource.units = units;
                    resource.content_units = content_units;
                    resource.rect = rect;
                    resource.paths = paths;
                }
            }
        }
        SVGNodeKind::Filter(data) => {
            if let Some(resource_id) = resource_graph.resource_id_for_node(node.tag.node) {
                let rect = svg_resource_rect(
                    data.x,
                    data.y,
                    data.width,
                    data.height,
                    -0.1,
                    -0.1,
                    1.2,
                    1.2,
                );
                if let Some(SVGResourceKind::Filter(resource)) =
                    resource_graph.resource_mut(resource_id)
                {
                    resource.rect = rect;
                }
            }
        }
        SVGNodeKind::Marker(data) => {
            if let Some(resource_id) = resource_graph.resource_id_for_node(node.tag.node) {
                let view_box = data.view_box.map(svg_rect_from_view_box);
                let marker_units = match data.marker_units {
                    Some(layout_api::SVGMarkerUnitsValue::UserSpaceOnUse) => {
                        SVGCoordinateUnits::UserSpaceOnUse
                    }
                    _ => SVGCoordinateUnits::UserSpaceOnUse,
                };
                let paths = collect_marker_paths(
                    node,
                    nodes_by_opaque,
                    resource_graph,
                    SVGTransform::identity(),
                );
                if let Some(SVGResourceKind::Marker(resource)) =
                    resource_graph.resource_mut(resource_id)
                {
                    resource.view_box = view_box;
                    resource.marker_units = marker_units;
                    resource.orient_auto = data.orient_auto;
                    resource.paths = paths;
                }
            }
        }
        _ => {}
    }

    for child in node.element_children() {
        resolve_svg_resource_node(child, nodes_by_opaque, resource_graph, visiting);
    }
}

#[derive(Clone)]
struct ResolvedPatternResourceData {
    units: SVGCoordinateUnits,
    content_units: SVGCoordinateUnits,
    pattern_transform: SVGTransform,
    rect: SVGPatternRect,
    view_box: Option<SVGRect>,
    preserve_aspect_ratio: SVGPreserveAspectRatio,
    content_source_node: Option<SVGResolvedNode>,
    source_resource_dependencies: Vec<SVGResourceId>,
}

fn resolve_pattern_resource_data(
    node: &SVGResolvedNode,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &SVGResourceGraph,
    visiting: &mut Vec<OpaqueNode>,
) -> Option<ResolvedPatternResourceData> {
    if visiting.contains(&node.tag.node) {
        return None;
    }
    visiting.push(node.tag.node);

    let template_node = pattern_template_node(node, nodes_by_opaque, resource_graph);
    let mut pattern = template_node
        .as_ref()
        .and_then(|template| {
            resolve_pattern_resource_data(template, nodes_by_opaque, resource_graph, visiting)
        })
        .unwrap_or_else(default_pattern_resource_data);

    if let Some(template_resource_id) = template_node
        .as_ref()
        .and_then(|template| resource_graph.resource_id_for_node(template.tag.node))
    {
        pattern.source_resource_dependencies.push(template_resource_id);
    }

    let SVGNodeKind::Pattern(data) = &node.svg_data().node_kind else {
        visiting.pop();
        return None;
    };

    if let Some(x) = data.x {
        pattern.rect.x = convert_svg_length_value(x);
    }
    if let Some(y) = data.y {
        pattern.rect.y = convert_svg_length_value(y);
    }
    if let Some(width) = data.width {
        pattern.rect.width = convert_svg_length_value(width);
    }
    if let Some(height) = data.height {
        pattern.rect.height = convert_svg_length_value(height);
    }
    if let Some(units) = data.pattern_units {
        pattern.units = units;
    }
    if let Some(content_units) = data.pattern_content_units {
        pattern.content_units = content_units;
    }
    if !data.pattern_transform.is_empty() {
        pattern.pattern_transform = parse_svg_transform(&data.pattern_transform);
    }
    if let Some(view_box) = data.view_box {
        pattern.view_box = Some(svg_rect_from_view_box(view_box));
    }
    if pattern_preserve_aspect_ratio_is_specified(node) {
        pattern.preserve_aspect_ratio = convert_svg_preserve_aspect_ratio(data.preserve_aspect_ratio);
    }
    if pattern_has_local_children(node) {
        pattern.content_source_node = Some(node.clone());
        pattern.source_resource_dependencies.extend(collect_pattern_content_resource_dependencies(
            node,
            nodes_by_opaque,
            resource_graph,
        ));
    }

    pattern.source_resource_dependencies.sort_by_key(|id| id.0);
    pattern.source_resource_dependencies.dedup();
    visiting.pop();
    Some(pattern)
}

fn default_pattern_resource_data() -> ResolvedPatternResourceData {
    ResolvedPatternResourceData {
        units: SVGCoordinateUnits::ObjectBoundingBox,
        content_units: SVGCoordinateUnits::UserSpaceOnUse,
        pattern_transform: SVGTransform::identity(),
        rect: SVGPatternRect::default(),
        view_box: None,
        preserve_aspect_ratio: SVGPreserveAspectRatio::default(),
        content_source_node: None,
        source_resource_dependencies: Vec::new(),
    }
}

fn pattern_has_local_children(node: &SVGResolvedNode) -> bool {
    node.element_children().next().is_some()
}

fn collect_pattern_content_resource_dependencies(
    node: &SVGResolvedNode,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &SVGResourceGraph,
) -> Vec<SVGResourceId> {
    let mut resources = Vec::new();
    for child in node.element_children() {
        collect_subtree_resource_dependencies_into(child, nodes_by_opaque, resource_graph, &mut resources);
    }
    resources.sort_by_key(|id| id.0);
    resources.dedup();
    resources
}

fn pattern_preserve_aspect_ratio_is_specified(node: &SVGResolvedNode) -> bool {
    node.metadata.preserve_aspect_ratio_specified
}

fn convert_svg_length_value(length: layout_api::SVGLengthValue) -> SVGLength {
    let unit = match length.unit_type {
        layout_api::SVG_LENGTHTYPE_PERCENTAGE => SVGLengthUnit::Percent,
        layout_api::SVG_LENGTHTYPE_PX => SVGLengthUnit::Px,
        layout_api::SVG_LENGTHTYPE_IN => SVGLengthUnit::In,
        layout_api::SVG_LENGTHTYPE_CM => SVGLengthUnit::Cm,
        layout_api::SVG_LENGTHTYPE_MM => SVGLengthUnit::Mm,
        layout_api::SVG_LENGTHTYPE_PT => SVGLengthUnit::Pt,
        layout_api::SVG_LENGTHTYPE_PC => SVGLengthUnit::Pc,
        _ => SVGLengthUnit::Number,
    };
    SVGLength {
        value: length.value,
        unit,
    }
}

fn convert_svg_preserve_aspect_ratio(
    value: layout_api::SVGPreserveAspectRatioValue,
) -> SVGPreserveAspectRatio {
    let align = match value.align {
        layout_api::SVG_PRESERVEASPECTRATIO_NONE => SVGPreserveAspectRatioAlign::None,
        layout_api::SVG_PRESERVEASPECTRATIO_XMINYMIN => SVGPreserveAspectRatioAlign::XMinYMin,
        layout_api::SVG_PRESERVEASPECTRATIO_XMIDYMIN => SVGPreserveAspectRatioAlign::XMidYMin,
        layout_api::SVG_PRESERVEASPECTRATIO_XMAXYMIN => SVGPreserveAspectRatioAlign::XMaxYMin,
        layout_api::SVG_PRESERVEASPECTRATIO_XMINYMID => SVGPreserveAspectRatioAlign::XMinYMid,
        layout_api::SVG_PRESERVEASPECTRATIO_XMIDYMID => SVGPreserveAspectRatioAlign::XMidYMid,
        layout_api::SVG_PRESERVEASPECTRATIO_XMAXYMID => SVGPreserveAspectRatioAlign::XMaxYMid,
        layout_api::SVG_PRESERVEASPECTRATIO_XMINYMAX => SVGPreserveAspectRatioAlign::XMinYMax,
        layout_api::SVG_PRESERVEASPECTRATIO_XMIDYMAX => SVGPreserveAspectRatioAlign::XMidYMax,
        layout_api::SVG_PRESERVEASPECTRATIO_XMAXYMAX => SVGPreserveAspectRatioAlign::XMaxYMax,
        _ => SVGPreserveAspectRatioAlign::XMidYMid,
    };
    let meet_or_slice = match value.meet_or_slice {
        layout_api::SVG_MEETORSLICE_SLICE => SVGMeetOrSlice::Slice,
        _ => SVGMeetOrSlice::Meet,
    };
    SVGPreserveAspectRatio {
        align,
        meet_or_slice,
    }
}

fn unpublish_resource_reference(node: &SVGResolvedNode, resource_graph: &mut SVGResourceGraph) {
    if let Some(element_id) = node.svg_data().common.element_id {
        resource_graph.resources_by_element_id.remove(element_id);
    }
}

fn resolve_gradient_resource(
    node: &SVGResolvedNode,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &SVGResourceGraph,
    visiting: &mut Vec<OpaqueNode>,
) -> Option<SVGGradientResource> {
    if visiting.contains(&node.tag.node) {
        return None;
    }
    visiting.push(node.tag.node);

    let template = gradient_template_node(node, nodes_by_opaque, resource_graph)
        .and_then(|template| {
            resolve_gradient_resource(template, nodes_by_opaque, resource_graph, visiting)
        });

    let mut gradient = template.unwrap_or_else(default_gradient_resource);
    match &node.svg_data().node_kind {
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
            gradient.units = (*gradient_units).unwrap_or(gradient.units);
            gradient.gradient_transform = parse_svg_transform(gradient_transform);
            gradient.spread_method = (*spread_method).unwrap_or(gradient.spread_method);
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
            gradient.units = (*gradient_units).unwrap_or(gradient.units);
            gradient.gradient_transform = parse_svg_transform(gradient_transform);
            gradient.spread_method = (*spread_method).unwrap_or(gradient.spread_method);
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

    let stops = collect_gradient_stops(node);
    if !stops.is_empty() {
        gradient.stops = stops;
    }

    visiting.pop();
    gradient.gradient_transform.is_invertible().then_some(gradient)
}

fn resolve_clip_path_resource(
    node: &SVGResolvedNode,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &SVGResourceGraph,
) -> SVGClipPathResource {
    let (units, transform) = match &node.svg_data().node_kind {
        SVGNodeKind::ClipPath(data) => (
            data.clip_path_units.unwrap_or(SVGCoordinateUnits::UserSpaceOnUse),
            parse_svg_transform(&node.svg_data().common.transform),
        ),
        _ => (SVGCoordinateUnits::UserSpaceOnUse, SVGTransform::identity()),
    };
    SVGClipPathResource {
        units,
        transform,
        paths: collect_clip_paths(
            node,
            nodes_by_opaque,
            resource_graph,
            SVGTransform::identity(),
        ),
    }
}

fn collect_clip_paths(
    node: &SVGResolvedNode,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &SVGResourceGraph,
    inherited_transform: SVGTransform,
) -> Vec<SVGPathData> {
    let node_transform = parse_svg_transform(&node.svg_data().common.transform);
    let combined_transform = then_svg_transform(inherited_transform, node_transform);
    match (&node.summary.kind, &node.svg_data().node_kind, &node.resolved_style) {
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
            .element_children()
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

fn collect_mask_paths(
    node: &SVGResolvedNode,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &SVGResourceGraph,
    inherited_transform: SVGTransform,
) -> Vec<SVGPathData> {
    let node_transform = parse_svg_transform(&node.svg_data().common.transform);
    let combined_transform = then_svg_transform(inherited_transform, node_transform);
    match (&node.summary.kind, &node.svg_data().node_kind, &node.resolved_style) {
        (
            SVGLayoutNodeKind::Geometry,
            SVGNodeKind::Geometry(geometry),
            SVGNodeResolvedStyle::Geometry(style),
        ) => {
            let path: SVGPathData = normalize_svg_geometry(geometry, style.fill_rule).into();
            if path.commands.is_empty() {
                Vec::new()
            } else {
                vec![transform_svg_path_data(&path, combined_transform)]
            }
        }
        (SVGLayoutNodeKind::Group, _, _) | (SVGLayoutNodeKind::Mask, _, _) => node
            .element_children()
            .flat_map(|child| {
                collect_mask_paths(child, nodes_by_opaque, resource_graph, combined_transform)
            })
            .collect(),
        (SVGLayoutNodeKind::Use, _, _) => {
            let expansion = expand_use_node(node, nodes_by_opaque, resource_graph);
            let use_transform = then_svg_transform(combined_transform, expansion.instance_transform);
            expansion
                .referenced_node
                .into_iter()
                .flat_map(|referenced| {
                    collect_mask_paths(referenced, nodes_by_opaque, resource_graph, use_transform)
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

fn collect_marker_paths(
    node: &SVGResolvedNode,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &SVGResourceGraph,
    inherited_transform: SVGTransform,
) -> Vec<havi_types::fragment_tree::SVGMarkerPathResource> {
    let node_transform = parse_svg_transform(&node.svg_data().common.transform);
    let combined_transform = then_svg_transform(inherited_transform, node_transform);
    match (&node.summary.kind, &node.svg_data().node_kind, &node.resolved_style) {
        (
            SVGLayoutNodeKind::Geometry,
            SVGNodeKind::Geometry(geometry),
            SVGNodeResolvedStyle::Geometry(style),
        ) => {
            let path: SVGPathData = normalize_svg_geometry(geometry, style.fill_rule).into();
            if path.commands.is_empty() {
                return Vec::new();
            }
            let path = transform_svg_path_data(&path, combined_transform);
            let object_bounding_box = path_bounds(&path).unwrap_or_default();
            let stroke_bounding_box = decorated_bounds(&path, style.paint.stroke.as_ref())
                .unwrap_or(object_bounding_box);
            vec![havi_types::fragment_tree::SVGMarkerPathResource {
                path,
                bounds: SVGBounds {
                    object_bounding_box,
                    stroke_bounding_box,
                    decorated_bounding_box: stroke_bounding_box,
                    visual_bounding_box: stroke_bounding_box,
                },
                paint: convert_paint_style(resource_graph, node.tag.node, &style.paint, style.opacity),
            }]
        }
        (SVGLayoutNodeKind::Group, _, _) | (SVGLayoutNodeKind::Marker, _, _) => node
            .element_children()
            .flat_map(|child| {
                collect_marker_paths(child, nodes_by_opaque, resource_graph, combined_transform)
            })
            .collect(),
        (SVGLayoutNodeKind::Use, _, _) => {
            let expansion = expand_use_node(node, nodes_by_opaque, resource_graph);
            let use_transform = then_svg_transform(combined_transform, expansion.instance_transform);
            expansion
                .referenced_node
                .into_iter()
                .flat_map(|referenced| {
                    collect_marker_paths(referenced, nodes_by_opaque, resource_graph, use_transform)
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

fn collect_gradient_stops(node: &SVGResolvedNode) -> Vec<SVGGradientStop> {
    node.element_children()
        .filter_map(|child| {
            let child_svg_data = child.svg_data();
            match &child_svg_data.node_kind {
                SVGNodeKind::Stop(stop) => Some(resolve_gradient_stop(stop, child)),
                _ => None,
            }
        })
        .collect()
}

fn resolve_gradient_stop(
    stop: &layout_api::SVGStopData<'_>,
    node: &SVGResolvedNode,
) -> SVGGradientStop {
    let color = match &node.resolved_style {
        SVGNodeResolvedStyle::Geometry(style) => style.paint.current_color,
        SVGNodeResolvedStyle::Viewport { geometry, .. } => geometry.paint.current_color,
        SVGNodeResolvedStyle::Text(text) => text.paint.current_color,
    };
    SVGGradientStop {
        offset: stop.offset.unwrap_or(0.0),
        color: stop
            .stop_color
            .map(str::to_owned)
            .as_deref()
            .and_then(parse_svg_color)
            .unwrap_or(color),
        opacity: stop
            .stop_opacity
            .map(str::to_owned)
            .as_deref()
            .and_then(|raw| layout_api::parse_svg_unit_interval(Some(raw)))
            .unwrap_or(1.0),
    }
}

fn gradient_template_node<'a>(
    node: &'a SVGResolvedNode,
    nodes_by_opaque: &'a SVGNodeMap<'a>,
    resource_graph: &SVGResourceGraph,
) -> Option<&'a SVGResolvedNode> {
    let href = match &node.svg_data().node_kind {
        SVGNodeKind::Gradient(SVGGradientData::Linear { href, .. })
        | SVGNodeKind::Gradient(SVGGradientData::Radial { href, .. }) => *href,
        _ => None,
    }?;
    let id = href.local_reference?;
    let target = resource_graph.node_for_element_id(id)?;
    let target = nodes_by_opaque.get(&target).copied()?;
    matches!(target.svg_data().node_kind, SVGNodeKind::Gradient(_)).then_some(target)
}

fn pattern_template_node<'a>(
    node: &'a SVGResolvedNode,
    nodes_by_opaque: &'a SVGNodeMap<'a>,
    resource_graph: &SVGResourceGraph,
) -> Option<&'a SVGResolvedNode> {
    let href = match &node.svg_data().node_kind {
        SVGNodeKind::Pattern(data) => data.href,
        _ => None,
    }?;
    let id = href.local_reference?;
    let target = resource_graph.node_for_element_id(id)?;
    let target = nodes_by_opaque.get(&target).copied()?;
    matches!(target.svg_data().node_kind, SVGNodeKind::Pattern(_)).then_some(target)
}

fn collect_subtree_resource_dependencies(
    node: &SVGResolvedNode,
    nodes_by_opaque: &SVGNodeMap<'_>,
    resource_graph: &SVGResourceGraph,
) -> Vec<SVGResourceId> {
    let mut resources = Vec::new();
    collect_subtree_resource_dependencies_into(node, nodes_by_opaque, resource_graph, &mut resources);
    resources.sort_by_key(|id| id.0);
    resources.dedup();
    resources
}

fn collect_subtree_resource_dependencies_into(
    node: &SVGResolvedNode,
    nodes_by_opaque: &SVGNodeMap<'_>,
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
    for child in node.element_children() {
        collect_subtree_resource_dependencies_into(child, nodes_by_opaque, resource_graph, resources);
    }
}

fn collect_resource_graph_node(
    node: &SVGResolvedNode,
    parent: Option<style::dom::OpaqueNode>,
    nodes: &mut Vec<SVGResourceGraphNode>,
) {
    let mut graph_node = SVGResourceGraphNode::new(node.tag.node, node.summary.kind);
    graph_node.parent = parent;
    graph_node.element_id = node.svg_data().common.element_id.map(str::to_owned);
    graph_node.establishes_viewport = node.summary.establishes_viewport;
    graph_node.participates_in_paint = node.summary.participates_in_paint;

    match (&node.svg_data().node_kind, &node.resolved_style) {
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

    graph_node.href = match &node.svg_data().node_kind {
        SVGNodeKind::Use(data) => data
            .href
            .and_then(|reference| reference.local_reference)
            .map(str::to_owned),
        SVGNodeKind::Gradient(SVGGradientData::Linear { href, .. })
        | SVGNodeKind::Gradient(SVGGradientData::Radial { href, .. }) => href
            .and_then(|reference| reference.local_reference)
            .map(str::to_owned),
        SVGNodeKind::Pattern(data) => data
            .href
            .and_then(|reference| reference.local_reference)
            .map(str::to_owned),
        SVGNodeKind::TextPath(data) => data
            .href
            .and_then(|reference| reference.local_reference)
            .map(str::to_owned),
        _ => None,
    };

    nodes.push(graph_node);
    for child in node.element_children() {
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
        SVGResolvedPaint::CurrentColor => SVGPaint::CurrentColor,
        SVGResolvedPaint::ContextFill => SVGPaint::ContextFill,
        SVGResolvedPaint::ContextStroke => SVGPaint::ContextStroke,
        SVGResolvedPaint::ResourceReference(reference) => {
            let iri = reference.iri.trim();
            let id = iri.strip_prefix('#').unwrap_or(iri);
            let resource_id = resource_graph.resource_for_element_id(id).filter(|id| {
                matches!(
                    resource_graph.resource(*id),
                    Some(SVGResourceKind::PaintServer(_))
                )
            });
            if let Some(resource_id) = resource_id {
                SVGPaint::Server(resource_id)
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
        dash_array: stroke.dash_array.clone(),
        dash_offset: stroke.dash_offset,
        vector_effect: stroke.vector_effect,
    }
}

fn convert_paint_style(
    resource_graph: &SVGResourceGraph,
    node: OpaqueNode,
    paint: &super::style::SVGPaintStyle,
    opacity: f32,
) -> SVGPaintStyle {
    SVGPaintStyle {
        fill: convert_resolved_paint(resource_graph, node, &paint.fill),
        fill_opacity: paint.fill_opacity,
        stroke: paint
            .stroke
            .as_ref()
            .map(|stroke| convert_stroke_style(resource_graph, node, stroke)),
        opacity,
        paint_order: paint.paint_order,
    }
}

fn convert_effect_state(resources: SVGEffectState) -> SVGEffectState {
    resources
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

fn svg_rect_from_view_box(view_box: layout_api::SVGRectValue) -> SVGRect {
    SVGRect::new(
        euclid::point2(view_box.x, view_box.y),
        euclid::size2(view_box.width, view_box.height),
    )
}

fn image_viewport(image: &layout_api::SVGImageData<'_>) -> SVGRect {
    SVGRect::new(
        euclid::point2(
            resolve_length(image.x).unwrap_or(0.0),
            resolve_length(image.y).unwrap_or(0.0),
        ),
        euclid::size2(
            resolve_length(image.width).unwrap_or(0.0),
            resolve_length(image.height).unwrap_or(0.0),
        ),
    )
}

fn image_bounds(viewport_rect: SVGRect, stroke: Option<&super::style::SVGResolvedStroke>) -> SVGBounds {
    let inflate = stroke.map(|stroke| stroke.width.max(0.0) * 0.5).unwrap_or(0.0);
    let mut stroke_bounding_box = viewport_rect;
    stroke_bounding_box.origin.x -= inflate;
    stroke_bounding_box.origin.y -= inflate;
    stroke_bounding_box.size.width += inflate * 2.0;
    stroke_bounding_box.size.height += inflate * 2.0;
    SVGBounds {
        object_bounding_box: viewport_rect,
        stroke_bounding_box,
        decorated_bounding_box: stroke_bounding_box,
        visual_bounding_box: stroke_bounding_box,
    }
}

fn union_fragment_rects(fragments: &[Fragment]) -> PhysicalRect<Au> {
    let mut rects = fragments.iter().map(Fragment::content_rect);
    let Some(first) = rects.next() else {
        return PhysicalRect::zero();
    };
    rects.fold(first, |union, rect| union.union(&rect))
}

fn svg_resource_rect(
    x: Option<layout_api::SVGLengthValue>,
    y: Option<layout_api::SVGLengthValue>,
    width: Option<layout_api::SVGLengthValue>,
    height: Option<layout_api::SVGLengthValue>,
    default_x: f32,
    default_y: f32,
    default_width: f32,
    default_height: f32,
) -> SVGRect {
    SVGRect::new(
        euclid::point2(
            parse_resource_rect_length(x, default_x),
            parse_resource_rect_length(y, default_y),
        ),
        euclid::size2(
            parse_resource_rect_length(width, default_width),
            parse_resource_rect_length(height, default_height),
        ),
    )
}

fn parse_resource_rect_length(length: Option<layout_api::SVGLengthValue>, default: f32) -> f32 {
    let Some(length) = length else {
        return default;
    };
    let value = match length.unit_type {
        layout_api::SVG_LENGTHTYPE_PERCENTAGE => length.value / 100.0,
        _ => resolve_length(Some(length)).unwrap_or(default),
    };
    if value.is_finite() { value } else { default }
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

fn parse_gradient_length(length: Option<layout_api::SVGLengthValue>, default: f32) -> f32 {
    let Some(length) = length else {
        return default;
    };
    let value = match length.unit_type {
        layout_api::SVG_LENGTHTYPE_PERCENTAGE => length.value / 100.0,
        _ => resolve_length(Some(length)).unwrap_or(default),
    };
    if value.is_finite() { value } else { default }
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


#[cfg(test)]
mod tests {
    use super::*;
    use havi_types::fragment_tree::{SVGGradientResource, SVGLinearGradient, SVGResourceNode};
    use layout_api::wrapper_traits::PseudoElementChain;
    use style::properties::ComputedValues;
    use style::properties::style_structs::Font;

    use crate::fragment_tree::Fragment;
    use crate::geom::{PhysicalPoint, PhysicalSize};
    use crate::svg::parse::parse_svg_tree;
    use crate::svg::style as svg_style;
    use crate::svg::tree::{SVGNodeData, SVGNodeId, SVGStandaloneTree, SVGTreeChild, SVGTreeNode};

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
            kind: SVGResourceKind::PaintServer(SVGPaintServerResource::Gradient(SVGGradientResource {
                units: SVGCoordinateUnits::ObjectBoundingBox,
                gradient_transform: SVGTransform::identity(),
                spread_method: SVGGradientSpreadMethod::Pad,
                kind: SVGGradientKind::Linear(SVGLinearGradient {
                    start: SVGPoint::new(0.0, 0.0),
                    end: SVGPoint::new(1.0, 0.0),
                }),
                stops: Vec::new(),
            })),
        });
        resource_graph
            .resources_by_element_id
            .insert("grad".to_owned(), SVGResourceId(0));

        let paint = convert_resolved_paint(
            &resource_graph,
            OpaqueNode(0),
            &SVGResolvedPaint::ResourceReference(svg_style::SVGPaintServerReference {
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
            SVGPaint::Server(SVGResourceId(0)) => {}
            other => panic!("expected paint server resource, got {other:?}"),
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

    fn initial_style() -> ServoArc<ComputedValues> {
        ComputedValues::initial_values_with_font_override(Font::initial_values()).to_arc()
    }

    fn number_length(value: f32) -> layout_api::SVGLengthValue {
        layout_api::SVGLengthValue {
            unit_type: layout_api::SVG_LENGTHTYPE_NUMBER,
            value,
        }
    }

    fn viewport_node(id: usize, width: Option<f32>, height: Option<f32>, view_box: Option<layout_api::SVGRectValue>, children: Vec<SVGTreeChild>) -> SVGTreeNode {
        SVGTreeNode::new(
            SVGNodeId(id),
            SVGNodeData::from(layout_api::SVGElementData {
                common: layout_api::SVGCommonData {
                    element_id: None,
                    transform: Vec::new(),
                },
                node_kind: SVGNodeKind::Viewport(layout_api::SVGViewportData {
                    width: width.map(number_length),
                    height: height.map(number_length),
                    view_box,
                    preserve_aspect_ratio: Default::default(),
                    overflow_hidden: false,
                }),
                paint: layout_api::SVGPaintData {
                    color: None,
                    fill: None,
                    fill_opacity: None,
                    fill_rule: None,
                    stroke: None,
                    stroke_opacity: None,
                    stroke_width: None,
                    stroke_linejoin: None,
                    stroke_linecap: None,
                    stroke_miterlimit: None,
                    stroke_dasharray: None,
                    stroke_dashoffset: None,
                    paint_order: None,
                    opacity: None,
                    pointer_events: None,
                    vector_effect: None,
                    clip_rule: None,
                    clip_path: None,
                    mask: None,
                    filter: None,
                    marker_start: None,
                    marker_mid: None,
                    marker_end: None,
                },
            }),
            children,
        )
    }

    fn group_node(id: usize, transform: &[layout_api::SVGTransformValue], children: Vec<SVGTreeChild>) -> SVGTreeNode {
        SVGTreeNode::new(
            SVGNodeId(id),
            SVGNodeData::from(layout_api::SVGElementData {
                common: layout_api::SVGCommonData {
                    element_id: None,
                    transform: transform.to_vec(),
                },
                node_kind: SVGNodeKind::Group,
                paint: layout_api::SVGPaintData {
                    color: None,
                    fill: None,
                    fill_opacity: None,
                    fill_rule: None,
                    stroke: None,
                    stroke_opacity: None,
                    stroke_width: None,
                    stroke_linejoin: None,
                    stroke_linecap: None,
                    stroke_miterlimit: None,
                    stroke_dasharray: None,
                    stroke_dashoffset: None,
                    paint_order: None,
                    opacity: None,
                    pointer_events: None,
                    vector_effect: None,
                    clip_rule: None,
                    clip_path: None,
                    mask: None,
                    filter: None,
                    marker_start: None,
                    marker_mid: None,
                    marker_end: None,
                },
            }),
            children,
        )
    }

    fn rect_node(id: usize, width: f32, height: f32, transform: &[layout_api::SVGTransformValue]) -> SVGTreeNode {
        SVGTreeNode::new(
            SVGNodeId(id),
            SVGNodeData::from(layout_api::SVGElementData {
                common: layout_api::SVGCommonData {
                    element_id: None,
                    transform: transform.to_vec(),
                },
                node_kind: SVGNodeKind::Geometry(layout_api::SVGGeometryData::Rect {
                    x: None,
                    y: None,
                    width: Some(number_length(width)),
                    height: Some(number_length(height)),
                    rx: None,
                    ry: None,
                }),
                paint: layout_api::SVGPaintData {
                    color: None,
                    fill: Some("green"),
                    fill_opacity: None,
                    fill_rule: None,
                    stroke: None,
                    stroke_opacity: None,
                    stroke_width: None,
                    stroke_linejoin: None,
                    stroke_linecap: None,
                    stroke_miterlimit: None,
                    stroke_dasharray: None,
                    stroke_dashoffset: None,
                    paint_order: None,
                    opacity: None,
                    pointer_events: None,
                    vector_effect: None,
                    clip_rule: None,
                    clip_path: None,
                    mask: None,
                    filter: None,
                    marker_start: None,
                    marker_mid: None,
                    marker_end: None,
                },
            }),
            Vec::new(),
        )
    }

    fn build_basic_standalone_fragment(tree: &SVGStandaloneTree, outer_rect: PhysicalRect<Au>) -> Fragment {
        let resolved = tree.resolve_with_default_style(initial_style());
        let nodes_by_opaque = build_resolved_node_map(&resolved);
        let mut resource_graph = build_svg_resource_graph(&resolved);
        resolve_svg_resources(&resolved, &nodes_by_opaque, &mut resource_graph);
        build_basic_viewport_fragment(&resolved, outer_rect, resource_graph, &nodes_by_opaque)
            .expect("basic standalone fragment")
    }

    fn build_basic_viewport_fragment(
        node: &SVGResolvedNode,
        rect: PhysicalRect<Au>,
        resource_graph: SVGResourceGraph,
        nodes_by_opaque: &SVGNodeMap<'_>,
    ) -> Option<Fragment> {
        let base_fragment_info = BaseFragmentInfo::new_for_testing(0);
        let style = initial_style();
        match (&node.summary.kind, &node.svg_data().node_kind, &node.resolved_style) {
            (
                SVGLayoutNodeKind::Viewport,
                SVGNodeKind::Viewport(viewport),
                SVGNodeResolvedStyle::Viewport { viewport: viewport_style, .. },
            ) => {
                let viewport_rect = svg_rect_from_physical_rect(rect);
                let view_box_rect = viewport.view_box.map(svg_rect_from_view_box);
                let mapper = compute_view_box_mapper(
                    viewport_rect,
                    view_box_rect,
                    viewport_style.preserve_aspect_ratio,
                );
                let viewport_transform = then_svg_transform(
                    mapper.local_to_parent,
                    parse_svg_transform(&node.svg_data().common.transform),
                );
                let children = node
                    .element_children()
                    .filter_map(|child| build_basic_child_fragment(child, &resource_graph, nodes_by_opaque))
                    .collect();
                Some(Fragment::SVGViewport(crate::cell::ArcRefCell::new(SVGViewportFragment {
                    base: BaseFragment::new(base_fragment_info, style.into(), rect),
                    identity: SVGFragmentIdentityContext::default().fragment_identity(node.tag),
                    children,
                    viewport_rect,
                    view_box_rect,
                    local_to_parent_transform: viewport_transform,
                    overflow_clip: None,
                    resource_graph: Some(resource_graph),
                    resource_owned_subtrees: Vec::new(),
                })))
            }
            _ => None,
        }
    }

    fn build_basic_child_fragment(
        node: &SVGResolvedNode,
        resource_graph: &SVGResourceGraph,
        nodes_by_opaque: &SVGNodeMap<'_>,
    ) -> Option<Fragment> {
        let resolved = resolved_node_resources(resource_graph, node.tag.node);
        let base_fragment_info = SVGFragmentIdentityContext::default().base_fragment_info(node.tag);
        let identity = SVGFragmentIdentityContext::default().fragment_identity(node.tag);
        match (&node.summary.kind, &node.svg_data().node_kind, &node.resolved_style) {
            (SVGLayoutNodeKind::Group, _, SVGNodeResolvedStyle::Geometry(_)) => {
                let children = node
                    .element_children()
                    .filter_map(|child| build_basic_child_fragment(child, resource_graph, nodes_by_opaque))
                    .collect::<Vec<_>>();
                let rect = union_fragment_rects(&children);
                Some(Fragment::SVGContainer(crate::cell::ArcRefCell::new(SVGContainerFragment {
                    base: BaseFragment::new(base_fragment_info, node.computed_style.clone().into(), rect),
                    identity,
                    kind: SVGContainerKind::Group,
                    children,
                    local_transform: parse_svg_transform(&node.svg_data().common.transform),
                    effects: convert_effect_state(resolved.resources.clone()),
                })))
            }
            (
                SVGLayoutNodeKind::Geometry,
                SVGNodeKind::Geometry(geometry),
                SVGNodeResolvedStyle::Geometry(style),
            ) => {
                let path: SVGPathData = normalize_svg_geometry(geometry, style.fill_rule).into();
                let transformed_path = transform_svg_path_data(
                    &path,
                    parse_svg_transform(&node.svg_data().common.transform),
                );
                let object_bounding_box = path_bounds(&transformed_path).unwrap_or_default();
                let stroke_bounding_box = decorated_bounds(&transformed_path, style.paint.stroke.as_ref())
                    .unwrap_or(object_bounding_box);
                let bounds = SVGBounds {
                    object_bounding_box,
                    stroke_bounding_box,
                    decorated_bounding_box: stroke_bounding_box,
                    visual_bounding_box: stroke_bounding_box,
                };
                Some(Fragment::SVGLeaf(crate::cell::ArcRefCell::new(SVGLeafFragment {
                    base: BaseFragment::new(
                        base_fragment_info,
                        node.computed_style.clone().into(),
                        physical_rect_from_svg_rect(bounds.visual_bounding_box),
                    ),
                    identity,
                    kind: SVGLeafKind::Path(SVGPathPayload {
                        path: transformed_path,
                    }),
                    bounds,
                    local_transform: SVGTransform::identity(),
                    paint: convert_paint_style(resource_graph, node.tag.node, &style.paint, style.opacity),
                    effects: convert_effect_state(resolved.resources.clone()),
                })))
            }
            (SVGLayoutNodeKind::Use, _, SVGNodeResolvedStyle::Geometry(_)) => {
                let expansion = expand_use_node(node, nodes_by_opaque, resource_graph);
                let children = expansion
                    .referenced_node
                    .into_iter()
                    .filter_map(|child| build_basic_child_fragment(child, resource_graph, nodes_by_opaque))
                    .collect::<Vec<_>>();
                let rect = union_fragment_rects(&children);
                Some(Fragment::SVGContainer(crate::cell::ArcRefCell::new(SVGContainerFragment {
                    base: BaseFragment::new(base_fragment_info, node.computed_style.clone().into(), rect),
                    identity,
                    kind: SVGContainerKind::Group,
                    children,
                    local_transform: expansion.instance_transform,
                    effects: convert_effect_state(resolved.resources.clone()),
                })))
            }
            _ => None,
        }
    }

    #[test]
    fn standalone_tree_builds_translated_leaf_fragment() {
        let tree = SVGStandaloneTree::new(viewport_node(
            0,
            Some(120.0),
            Some(80.0),
            Some(layout_api::SVGRectValue {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 80.0,
            }),
            vec![SVGTreeChild::Node(rect_node(
                1,
                40.0,
                30.0,
                &layout_api::parse_svg_transform_list(Some("translate(20 10)")),
            ))],
        ));
        let fragment = build_basic_standalone_fragment(
            &tree,
            PhysicalRect::new(PhysicalPoint::zero(), PhysicalSize::new(Au::from_px(120), Au::from_px(80))),
        );
        let Fragment::SVGViewport(viewport) = &fragment else {
            panic!("expected SVG viewport");
        };
        let viewport = viewport.borrow();
        assert_eq!(viewport.children.len(), 1);
        let Fragment::SVGLeaf(leaf) = &viewport.children[0] else {
            panic!("expected SVG leaf child");
        };
        let leaf = leaf.borrow();
        assert_eq!(leaf.bounds.visual_bounding_box.origin.x, 20.0);
        assert_eq!(leaf.bounds.visual_bounding_box.origin.y, 10.0);
        assert_eq!(leaf.bounds.visual_bounding_box.size.width, 40.0);
        assert_eq!(leaf.bounds.visual_bounding_box.size.height, 30.0);
    }

    #[test]
    fn standalone_tree_builds_group_container_hierarchy() {
        let tree = SVGStandaloneTree::new(viewport_node(
            0,
            Some(100.0),
            Some(100.0),
            Some(layout_api::SVGRectValue {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            }),
            vec![SVGTreeChild::Node(group_node(
                1,
                &layout_api::parse_svg_transform_list(Some("translate(5 7)")),
                vec![SVGTreeChild::Node(rect_node(2, 10.0, 12.0, &[]))],
            ))],
        ));
        let fragment = build_basic_standalone_fragment(
            &tree,
            PhysicalRect::new(PhysicalPoint::zero(), PhysicalSize::new(Au::from_px(100), Au::from_px(100))),
        );
        let Fragment::SVGViewport(viewport) = &fragment else {
            panic!("expected SVG viewport");
        };
        let viewport = viewport.borrow();
        let Fragment::SVGContainer(container) = &viewport.children[0] else {
            panic!("expected SVG container child");
        };
        let container = container.borrow();
        assert_eq!(container.children.len(), 1);
        assert_eq!(container.local_transform.m31, 5.0);
        assert_eq!(container.local_transform.m32, 7.0);
    }

    #[test]
    fn standalone_tree_entrypoint_is_exposed() {
        let _ = build_svg_root_fragment_from_tree;
    }

    #[test]
    fn non_invertible_gradient_transform_unpublishes_resource_and_uses_fallback() {
        let tree = parse_svg_tree(br#"
            <svg xmlns="http://www.w3.org/2000/svg">
              <defs>
                <linearGradient id="g" gradientTransform="scale(0)">
                  <stop offset="0" stop-color="yellow"/>
                  <stop offset="1" stop-color="red"/>
                </linearGradient>
              </defs>
              <rect width="10" height="10" fill="url(#g) green"/>
            </svg>
        "#)
        .expect("svg tree");
        let resolved = tree.resolve_with_default_style(initial_style());
        let nodes_by_opaque = build_resolved_node_map(&resolved);
        let mut resource_graph = build_svg_resource_graph(&resolved);
        resolve_svg_resources(&resolved, &nodes_by_opaque, &mut resource_graph);

        assert!(resource_graph.resource_for_element_id("g").is_none());

        let rect = resolved
            .element_children()
            .find(|child| matches!(child.summary.kind, SVGLayoutNodeKind::Geometry))
            .expect("rect child");
        let SVGNodeResolvedStyle::Geometry(style) = &rect.resolved_style else {
            panic!("expected geometry style");
        };
        let paint = convert_resolved_paint(&resource_graph, rect.tag.node, &style.paint.fill);
        let SVGPaint::SolidColor(color) = paint else {
            panic!("expected fallback solid color");
        };
        assert_eq!(color.red, 0.0);
        assert_eq!(color.green, 128.0 / 255.0);
        assert_eq!(color.blue, 0.0);
        assert_eq!(color.alpha, 1.0);
    }

    #[test]
    fn intrinsic_sizes_fall_back_to_viewbox_dimensions() {
        let svg_data = layout_api::SVGElementData {
            common: layout_api::SVGCommonData {
                element_id: None,
                transform: Vec::new(),
            },
            node_kind: SVGNodeKind::Viewport(layout_api::SVGViewportData {
                width: None,
                height: None,
                view_box: Some(layout_api::SVGRectValue {
                    x: 0.0,
                    y: 0.0,
                    width: 60.0,
                    height: 30.0,
                }),
                preserve_aspect_ratio: Default::default(),
                overflow_hidden: false,
            }),
            paint: layout_api::SVGPaintData {
                color: None,
                fill: None,
                fill_opacity: None,
                fill_rule: None,
                stroke: None,
                stroke_opacity: None,
                stroke_width: None,
                stroke_linejoin: None,
                stroke_linecap: None,
                stroke_miterlimit: None,
                stroke_dasharray: None,
                stroke_dashoffset: None,
                paint_order: None,
                opacity: None,
                pointer_events: None,
                vector_effect: None,
                clip_rule: None,
                clip_path: None,
                mask: None,
                filter: None,
                marker_start: None,
                marker_mid: None,
                marker_end: None,
            },
        };
        let sizes = intrinsic_svg_root_sizes(&svg_data);
        assert_eq!(sizes.width, Some(Au::from_px(60)));
        assert_eq!(sizes.height, Some(Au::from_px(30)));
        assert_eq!(sizes.ratio, Some(2.0));
    }
}
