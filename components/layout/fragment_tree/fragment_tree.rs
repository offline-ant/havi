/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::sync::Arc;

use app_units::Au;
use base::print_tree::PrintTree;
use bitflags::bitflags;
use havi_types::fragment_tree as published;
use rustc_hash::FxHashSet;
use style::animation::AnimationSetKey;
use style::computed_values::position::T as Position;
use style::values::computed::Image;
use style::values::specified::Overflow;

use super::{
    BaseFragmentStyleRef, ContainingBlockManager, Fragment, ImageFragment,
    OutOfFlowPlacementFragment, SpecificLayoutInfo, TextFragment,
};
use crate::context::{ImageResolver, LayoutContext};
use crate::fragment_tree::{BaseFragmentInfo, FragmentFlags};
use crate::geom::{PhysicalPoint, PhysicalRect, PhysicalSize};
use crate::positioned::PositioningContext;
use crate::svg::layout::build_svg_root_fragment_from_tree_with_text_context;
use crate::svg::parse::{
    compute_svg_image_intrinsic_sizes, extract_svg_root_metadata, parse_svg_tree,
};
use crate::svg::text::SVGTextLayoutContext;

/// A scroll type, describing what kind of action originated a scroll request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollType(u8);

bitflags! {
    impl ScrollType: u8 {
        const InputEvents = 1 << 0;
        const Script = 1 << 1;
    }
}

impl From<Overflow> for ScrollType {
    fn from(overflow: Overflow) -> Self {
        match overflow {
            Overflow::Hidden => ScrollType::Script,
            Overflow::Scroll | Overflow::Auto => ScrollType::Script | ScrollType::InputEvents,
            Overflow::Visible | Overflow::Clip => ScrollType::empty(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxesScrollSensitivity {
    pub x: ScrollType,
    pub y: ScrollType,
}

pub struct FragmentTree {
    generation: Arc<published::FragmentArenaGeneration>,
    svg_text_layout_context: SVGTextLayoutContext,
    pub viewport_scroll_sensitivity: AxesScrollSensitivity,
}

impl FragmentTree {
    pub(crate) fn new(
        layout_context: &LayoutContext,
        root_fragments: Vec<Fragment>,
        initial_containing_block: PhysicalRect<Au>,
        viewport_scroll_sensitivity: AxesScrollSensitivity,
    ) -> Self {
        let root_fragments: Arc<[Fragment]> = root_fragments.into();
        let containing_blocks = collect_containing_blocks(root_fragments.as_ref(), &initial_containing_block);

        let mut animations = layout_context.style_context.animations.sets.write();
        let mut invalid_animating_nodes: FxHashSet<_> = animations.keys().cloned().collect();

        let mut animating_images = layout_context.image_resolver.animating_images.write();
        let mut invalid_image_animating_nodes: FxHashSet<_> = animating_images
            .node_to_state_map
            .keys()
            .cloned()
            .map(|node| AnimationSetKey::new(node, None))
            .collect();

        for fragment in root_fragments.iter() {
            fragment.find(
                &ContainingBlockManager {
                    for_non_absolute_descendants: &initial_containing_block,
                    for_absolute_descendants: None,
                    for_absolute_and_fixed_descendants: &initial_containing_block,
                },
                0,
                &mut |fragment, _level, containing_block| {
                    if let Some(tag) = fragment.tag() {
                        invalid_animating_nodes.remove(&AnimationSetKey::new(
                            tag.node,
                            tag.pseudo_element_chain.primary,
                        ));
                        invalid_image_animating_nodes.remove(&AnimationSetKey::new(
                            tag.node,
                            tag.pseudo_element_chain.primary,
                        ));
                    }
                    fragment.set_containing_block(containing_block);
                    None::<()>
                },
            );
        }

        for fragment in root_fragments.iter() {
            fragment.calculate_scrollable_overflow();
        }

        let scrollable_overflow = compute_tree_scrollable_overflow(
            root_fragments.as_ref(),
            initial_containing_block,
        );

        let svg_text_layout_context = SVGTextLayoutContext::from(layout_context);
        let generation = Arc::new(build_generation(
            root_fragments.as_ref(),
            &containing_blocks,
            initial_containing_block,
            scrollable_overflow,
            &layout_context.image_resolver,
            &svg_text_layout_context,
        ));

        for node in &invalid_animating_nodes {
            if let Some(state) = animations.get_mut(node) {
                state.cancel_all_animations();
            }
        }
        for node in &invalid_image_animating_nodes {
            animating_images.remove(node.node);
        }
        drop(animating_images);
        drop(animations);

        Self {
            generation,
            svg_text_layout_context,
            viewport_scroll_sensitivity,
        }
    }

    pub fn generation(&self) -> Arc<published::FragmentArenaGeneration> {
        self.generation.clone()
    }

    pub fn print(&self) {
        let mut print_tree = PrintTree::new("Fragment Tree".to_string());
        for root in self.generation.geometry_roots.iter().copied() {
            print_generation_fragment(&self.generation, root, &mut print_tree);
        }
    }

    pub(crate) fn refresh_background_images(
        &self,
        image_resolver: &Arc<ImageResolver>,
    ) -> Self {
        let mut derived = self.generation.derived.clone();
        derived.background_images = self
            .generation
            .nodes
            .iter()
            .map(|node| match &node.kind {
                published::FragmentKind::Box(_) | published::FragmentKind::Float(_) => {
                    resolve_background_images_for_base(
                        node.base(),
                        image_resolver,
                        &self.svg_text_layout_context,
                    )
                }
                _ => Vec::new(),
            })
            .collect();

        Self {
            generation: Arc::new(published::FragmentArenaGeneration {
                geometry_roots: self.generation.geometry_roots.clone(),
                paint_roots: self.generation.paint_roots.clone(),
                nodes: self.generation.nodes.clone(),
                placements: self.generation.placements.clone(),
                derived,
                node_fragments: self.generation.node_fragments.clone(),
                svg_resources: self.generation.svg_resources.clone(),
                initial_containing_block: self.generation.initial_containing_block,
                scrollable_overflow: self.generation.scrollable_overflow,
                document_canvas_background: self.generation.document_canvas_background.clone(),
            }),
            svg_text_layout_context: self.svg_text_layout_context.clone(),
            viewport_scroll_sensitivity: self.viewport_scroll_sensitivity,
        }
    }

    pub(crate) fn scrollable_overflow(&self) -> PhysicalRect<Au> {
        self.generation.scrollable_overflow
    }

    pub(crate) fn fragments_for_node(
        &self,
        node: style::dom::OpaqueNode,
        pseudo: Option<style::selector_parser::PseudoElement>,
    ) -> &[published::FragmentId] {
        self.generation.fragments_for_node(node, pseudo)
    }
}

fn build_generation(
    root_fragments: &[Fragment],
    containing_blocks: &HashMap<usize, PhysicalRect<Au>>,
    initial_containing_block: PhysicalRect<Au>,
    scrollable_overflow: PhysicalRect<Au>,
    image_resolver: &Arc<ImageResolver>,
    svg_text_layout_context: &SVGTextLayoutContext,
) -> published::FragmentArenaGeneration {
    let mut builder = ArenaBuilder::new(containing_blocks, image_resolver, svg_text_layout_context);
    let geometry_roots: Vec<_> = root_fragments
        .iter()
        .filter_map(|fragment| builder.build_geometry(fragment, None, None, true))
        .collect();
    builder.populate_paint_children(root_fragments);

    let paint_roots = root_fragments
        .iter()
        .filter_map(|fragment| builder.paint_child_for_root(fragment))
        .collect::<Vec<_>>();

    let document_canvas_background = builder.resolve_document_canvas_background(initial_containing_block);

    published::FragmentArenaGeneration {
        geometry_roots: Arc::from(geometry_roots),
        paint_roots: Arc::from(paint_roots),
        nodes: Arc::from(builder.nodes),
        placements: Arc::from(builder.placements),
        derived: builder.derived,
        node_fragments: builder
            .node_fragments
            .into_iter()
            .map(|(key, ids)| (key, Arc::from(ids)))
            .collect(),
        svg_resources: Arc::from(builder.svg_resources),
        initial_containing_block,
        scrollable_overflow,
        document_canvas_background,
    }
}

struct ArenaBuilder<'a> {
    containing_blocks: &'a HashMap<usize, PhysicalRect<Au>>,
    image_resolver: &'a Arc<ImageResolver>,
    svg_text_layout_context: &'a SVGTextLayoutContext,
    nodes: Vec<published::FragmentNode>,
    placements: Vec<published::OutOfFlowPlacement>,
    derived: published::FragmentDerivedData,
    internal_to_node: HashMap<usize, published::FragmentId>,
    node_fragments: HashMap<published::FragmentMapKey, Vec<published::FragmentId>>,
    placement_targets: HashMap<u32, published::FragmentId>,
    placement_ids: HashMap<u32, published::PlacementId>,
    svg_resources: Vec<published::SVGResourceNode>,
}

impl<'a> ArenaBuilder<'a> {
    fn new(
        containing_blocks: &'a HashMap<usize, PhysicalRect<Au>>,
        image_resolver: &'a Arc<ImageResolver>,
        svg_text_layout_context: &'a SVGTextLayoutContext,
    ) -> Self {
        Self {
            containing_blocks,
            image_resolver,
            svg_text_layout_context,
            nodes: Vec::new(),
            placements: Vec::new(),
            derived: published::FragmentDerivedData {
                containing_blocks: Vec::new(),
                scrollable_overflow: Vec::new(),
                sticky_insets: Vec::new(),
                background_images: Vec::new(),
                suppress_background_paint: Vec::new(),
            },
            internal_to_node: HashMap::new(),
            node_fragments: HashMap::new(),
            placement_targets: HashMap::new(),
            placement_ids: HashMap::new(),
            svg_resources: Vec::new(),
        }
    }

    fn build_geometry(
        &mut self,
        fragment: &Fragment,
        parent: Option<published::FragmentId>,
        resource_offset: Option<u32>,
        record_mapping: bool,
    ) -> Option<published::FragmentId> {
        let key = internal_fragment_key(fragment)?;
        if let Some(id) = self.internal_to_node.get(&key).copied() {
            return Some(id);
        }

        // Reserve the arena slot before processing children. Children call
        // build_geometry recursively and use self.nodes.len() to compute their
        // own ids. Without this reservation the first child would see the same
        // len as its parent and receive a duplicate FragmentId.
        let id = published::FragmentId(self.nodes.len() as u32);
        self.internal_to_node.insert(key, id);
        let base = convert_fragment_base(fragment);
        if record_mapping {
            self.record_node_mapping(id, published_fragment_mapping_tag(fragment, &base));
        }
        self.push_derived(fragment, &base);
        if let Some(placement_id) = fragment_out_of_flow_placement_id(fragment) {
            self.placement_targets.insert(placement_id, id);
        }
        // Placeholder node to reserve the slot. The kind is never read;
        // it is overwritten at the end of this function after children
        // are processed. Only .parent matters (ensure_placement reads it).
        self.nodes.push(published::FragmentNode {
            parent,
            kind: published::FragmentKind::Positioning(published::PositioningFragment {
                base: base.clone(),
                geometry_children: Vec::new(),
                paint_children: Vec::new(),
            }),
        });

        // Build the real kind. Container types recurse into children here;
        // those children now correctly see self.nodes.len() > id.
        let kind = match fragment {
            Fragment::Box(box_fragment) | Fragment::Float(box_fragment) => {
                let box_fragment = box_fragment.borrow();
                let geometry_children = box_fragment
                    .children
                    .iter()
                    .filter_map(|child| {
                        self.build_geometry(child, Some(id), resource_offset, record_mapping)
                    })
                    .collect();
                let specific_layout_info = convert_specific_layout_info(box_fragment.specific_layout_info());
                let node = published::BoxFragment {
                    base,
                    geometry_children,
                    paint_children: Vec::new(),
                    padding: box_fragment.padding,
                    border: box_fragment.border,
                    margin: box_fragment.margin,
                    baselines: {
                        let baselines = box_fragment.baselines(box_fragment.base.style().writing_mode);
                        published::Baselines {
                            first: baselines.first,
                            last: baselines.last,
                        }
                    },
                    specific_layout_info,
                    block_level_info: box_fragment.block_level_layout_info.as_ref().map(|info| {
                        Box::new(published::BlockLevelLayoutInfo {
                            clearance: info.clearance,
                            block_margins_collapsed_with_children: published::CollapsedBlockMargins {
                                collapsed_through: info.block_margins_collapsed_with_children.collapsed_through,
                                start: convert_collapsed_margin(info.block_margins_collapsed_with_children.start),
                                end: convert_collapsed_margin(info.block_margins_collapsed_with_children.end),
                            },
                        })
                    }),
                };
                if matches!(fragment, Fragment::Float(_)) {
                    published::FragmentKind::Float(node)
                } else {
                    published::FragmentKind::Box(node)
                }
            }
            Fragment::Positioning(positioning_fragment) => {
                let positioning_fragment = positioning_fragment.borrow();
                let geometry_children = positioning_fragment
                    .children
                    .iter()
                    .filter_map(|child| {
                        self.build_geometry(child, Some(id), resource_offset, record_mapping)
                    })
                    .collect();
                published::FragmentKind::Positioning(published::PositioningFragment {
                    base,
                    geometry_children,
                    paint_children: Vec::new(),
                })
            }
            Fragment::Text(text_fragment) => {
                let text_fragment = text_fragment.borrow();
                published::FragmentKind::Text(convert_text_fragment(&text_fragment))
            }
            Fragment::Image(image_fragment) => {
                let image_fragment = image_fragment.borrow();
                published::FragmentKind::Image(convert_image_fragment(&image_fragment))
            }
            Fragment::IFrame(iframe_fragment) => {
                let iframe_fragment = iframe_fragment.borrow();
                published::FragmentKind::IFrame(published::IFrameFragment {
                    base: convert_base(&iframe_fragment.base),
                    pipeline_id: iframe_fragment.pipeline_id,
                })
            }
            Fragment::SVGViewport(svg_fragment) => {
                let svg_fragment = svg_fragment.borrow();
                let resource_offset = svg_fragment.resource_graph.as_ref().map(|graph| {
                    let offset = self.svg_resources.len() as u32;
                    self.svg_resources.extend(
                        graph.resources()
                            .iter()
                            .cloned()
                            .map(|resource| remap_svg_resource_node(resource, offset)),
                    );
                    offset
                }).or(resource_offset);
                let geometry_children = svg_fragment
                    .children
                    .iter()
                    .filter_map(|child| {
                        self.build_geometry(child, Some(id), resource_offset, record_mapping)
                    })
                    .collect();
                if let Some(resource_offset) = resource_offset {
                    self.publish_svg_resource_owned_subtrees(
                        &svg_fragment.resource_owned_subtrees,
                        resource_offset,
                    );
                }
                published::FragmentKind::SVGViewport(published::SVGViewportFragment {
                    base,
                    identity: svg_fragment.identity.clone(),
                    geometry_children,
                    paint_children: Vec::new(),
                    viewport_rect: svg_fragment.viewport_rect,
                    view_box_rect: svg_fragment.view_box_rect,
                    local_to_parent_transform: svg_fragment.local_to_parent_transform,
                    overflow_clip: svg_fragment.overflow_clip.clone(),
                })
            }
            Fragment::SVGContainer(svg_fragment) => {
                let svg_fragment = svg_fragment.borrow();
                let geometry_children = svg_fragment
                    .children
                    .iter()
                    .filter_map(|child| {
                        self.build_geometry(child, Some(id), resource_offset, record_mapping)
                    })
                    .collect();
                published::FragmentKind::SVGContainer(published::SVGContainerFragment {
                    base,
                    identity: svg_fragment.identity.clone(),
                    kind: svg_fragment.kind.clone(),
                    geometry_children,
                    paint_children: Vec::new(),
                    local_transform: svg_fragment.local_transform,
                    effects: remap_svg_effect_state(svg_fragment.effects.clone(), resource_offset),
                })
            }
            Fragment::SVGLeaf(svg_fragment) => {
                let svg_fragment = svg_fragment.borrow();
                published::FragmentKind::SVGLeaf(published::SVGLeafFragment {
                    base,
                    identity: svg_fragment.identity.clone(),
                    kind: remap_svg_leaf_kind(svg_fragment.kind.clone(), resource_offset),
                    bounds: remap_svg_bounds(svg_fragment.bounds.clone(), resource_offset),
                    local_transform: svg_fragment.local_transform,
                    paint: remap_svg_paint_style(svg_fragment.paint.clone(), resource_offset),
                    effects: remap_svg_effect_state(svg_fragment.effects.clone(), resource_offset),
                })
            }
            Fragment::AbsoluteOrFixedPositioned(_) => unreachable!("filtered by internal_fragment_key"),
        };

        self.nodes[id.0 as usize] = published::FragmentNode { parent, kind };
        Some(id)
    }

    fn publish_svg_resource_owned_subtrees(
        &mut self,
        subtrees: &[super::SVGResourceOwnedSubtree],
        resource_offset: u32,
    ) {
        for subtree in subtrees {
            let fragment_roots = subtree
                .fragment_roots
                .iter()
                .filter_map(|fragment| self.build_geometry(fragment, None, Some(resource_offset), false))
                .collect::<Vec<_>>();
            let resource_dependencies = subtree
                .resource_dependencies
                .iter()
                .copied()
                .map(|id| remap_svg_resource_id(id, Some(resource_offset)))
                .collect::<Vec<_>>();
            let resource_id = remap_svg_resource_id(subtree.owner_resource_id, Some(resource_offset));
            let Some(resource) = self.svg_resources.get_mut(resource_id.0 as usize) else {
                continue;
            };
            match &mut resource.kind {
                published::SVGResourceKind::PaintServer(
                    published::SVGPaintServerResource::Pattern(pattern),
                ) => {
                    pattern.source_fragment_roots = fragment_roots;
                    pattern.source_resource_dependencies = resource_dependencies;
                }
                _ => {}
            }
        }
    }

    fn push_derived(&mut self, fragment: &Fragment, base: &published::BaseFragment) {
        let containing_block = self
            .containing_blocks
            .get(&internal_fragment_key(fragment).expect("actual fragments have stable keys"))
            .copied()
            .unwrap_or_default();
        self.derived.containing_blocks.push(containing_block);
        self.derived.scrollable_overflow.push(match fragment {
            Fragment::Box(box_fragment) | Fragment::Float(box_fragment) => {
                box_fragment.borrow().scrollable_overflow()
            }
            Fragment::Positioning(positioning_fragment) => {
                positioning_fragment.borrow().scrollable_overflow_for_parent()
            }
            Fragment::Text(_) |
            Fragment::Image(_) |
            Fragment::IFrame(_) |
            Fragment::SVGViewport(_) |
            Fragment::SVGContainer(_) |
            Fragment::SVGLeaf(_) => base.rect,
            Fragment::AbsoluteOrFixedPositioned(_) => PhysicalRect::zero(),
        });
        self.derived.sticky_insets.push(match fragment {
            Fragment::Box(box_fragment) | Fragment::Float(box_fragment) => box_fragment
                .borrow()
                .resolved_sticky_insets
                .borrow()
                .clone(),
            _ => None,
        });
        self.derived.background_images.push(match fragment {
            Fragment::Box(_) | Fragment::Float(_) => resolve_background_images_for_base(
                base,
                self.image_resolver,
                self.svg_text_layout_context,
            ),
            _ => Vec::new(),
        });
        self.derived.suppress_background_paint.push(false);
    }

    fn resolve_document_canvas_background(
        &mut self,
        initial_containing_block: PhysicalRect<Au>,
    ) -> Option<published::DocumentCanvasBackground> {
        let root_fragment_id = self.first_box_fragment_with_flag(published::FragmentFlags::IS_ROOT_ELEMENT)?;
        let root_base = self.nodes[root_fragment_id.0 as usize].base();
        let root_background_visible = fragment_supplies_canvas_background(root_base);
        let document_canvas_background = if root_background_visible {
            Some(published::DocumentCanvasBackground {
                source_kind: published::CanvasBackgroundSource::RootElement,
                source_fragment_id: root_fragment_id,
                paint_rect: initial_containing_block,
            })
        } else if root_allows_body_canvas_background(root_base) {
            self.first_box_fragment_with_flag(
                published::FragmentFlags::IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT,
            )
            .filter(|body_fragment_id| {
                fragment_supplies_canvas_background(self.nodes[body_fragment_id.0 as usize].base())
            })
            .map(|source_fragment_id| published::DocumentCanvasBackground {
                source_kind: published::CanvasBackgroundSource::PropagatedHtmlBody,
                source_fragment_id,
                paint_rect: initial_containing_block,
            })
        } else {
            None
        };

        if let Some(canvas_background) = &document_canvas_background {
            self.suppress_background_paint_for_source(canvas_background.source_fragment_id);
        }

        document_canvas_background
    }

    fn suppress_background_paint_for_source(&mut self, source_fragment_id: published::FragmentId) {
        let source_base = self.nodes[source_fragment_id.0 as usize].base();
        let suppress_flag = if source_base
            .flags
            .contains(published::FragmentFlags::IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT)
        {
            Some(published::FragmentFlags::IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT)
        } else if source_base.flags.contains(published::FragmentFlags::IS_ROOT_ELEMENT) {
            Some(published::FragmentFlags::IS_ROOT_ELEMENT)
        } else {
            None
        };
        let source_tag = source_base.tag;

        for (index, node) in self.nodes.iter().enumerate() {
            let fragment_id = published::FragmentId(index as u32);
            let node_base = node.base();
            let same_anonymous_style = node_base.tag.is_none()
                && self.fragment_is_descendant_of(fragment_id, source_fragment_id)
                && node_base.style.get_background() == source_base.style.get_background();
            if !matches!(
                &node.kind,
                published::FragmentKind::Box(_) | published::FragmentKind::Float(_)
            ) {
                continue;
            }
            let same_flag = suppress_flag.is_some_and(|flag| node_base.flags.contains(flag));
            let same_tag = source_tag.is_some() && node_base.tag == source_tag;
            if same_flag || same_tag || same_anonymous_style {
                self.derived.suppress_background_paint[index] = true;
            }
        }
    }

    fn fragment_is_descendant_of(
        &self,
        fragment_id: published::FragmentId,
        ancestor_id: published::FragmentId,
    ) -> bool {
        let mut current = self.nodes[fragment_id.0 as usize].parent;
        while let Some(parent) = current {
            if parent == ancestor_id {
                return true;
            }
            current = self.nodes[parent.0 as usize].parent;
        }
        false
    }

    fn first_box_fragment_with_flag(
        &self,
        flag: published::FragmentFlags,
    ) -> Option<published::FragmentId> {
        self.nodes.iter().enumerate().find_map(|(index, node)| {
            matches!(
                &node.kind,
                published::FragmentKind::Box(_) | published::FragmentKind::Float(_)
            )
            .then_some(node.base())
            .filter(|base| base.flags.contains(flag))
            .map(|_| published::FragmentId(index as u32))
        })
    }

    fn record_node_mapping(&mut self, id: published::FragmentId, tag: Option<published::Tag>) {
        let Some(tag) = tag else {
            return;
        };
        self.node_fragments
            .entry(published::FragmentMapKey {
                node: tag.node,
                pseudo: tag.pseudo,
            })
            .or_default()
            .push(id);
    }

    fn populate_paint_children(&mut self, root_fragments: &[Fragment]) {
        for fragment in root_fragments {
            self.populate_fragment_paint_children(fragment);
        }
    }

    fn populate_fragment_paint_children(&mut self, fragment: &Fragment) {
        let Some(id) = internal_fragment_key(fragment).and_then(|key| self.internal_to_node.get(&key).copied()) else {
            return;
        };

        let children = match fragment {
            Fragment::Box(box_fragment) | Fragment::Float(box_fragment) => box_fragment.borrow().children.clone(),
            Fragment::Positioning(positioning_fragment) => positioning_fragment.borrow().children.clone(),
            Fragment::SVGViewport(svg_fragment) => svg_fragment.borrow().children.clone(),
            Fragment::SVGContainer(svg_fragment) => svg_fragment.borrow().children.clone(),
            Fragment::Text(_) |
            Fragment::Image(_) |
            Fragment::IFrame(_) |
            Fragment::SVGLeaf(_) => Vec::new(),
            Fragment::AbsoluteOrFixedPositioned(_) => return,
        };

        let mut paint_children = Vec::new();
        for child in &children {
            if let Some(paint_child) = self.paint_child_for_subtree(child) {
                paint_children.push(paint_child);
            }
            self.populate_fragment_paint_children(child);
        }

        match &mut self.nodes[id.0 as usize].kind {
            published::FragmentKind::Box(box_fragment) | published::FragmentKind::Float(box_fragment) => {
                box_fragment.paint_children = paint_children;
            }
            published::FragmentKind::Positioning(positioning_fragment) => {
                positioning_fragment.paint_children = paint_children;
            }
            published::FragmentKind::SVGViewport(svg_fragment) => {
                svg_fragment.paint_children = paint_children;
            }
            published::FragmentKind::SVGContainer(svg_fragment) => {
                svg_fragment.paint_children = paint_children;
            }
            published::FragmentKind::Text(_) |
            published::FragmentKind::Image(_) |
            published::FragmentKind::IFrame(_) |
            published::FragmentKind::SVGLeaf(_) => {}
        }
    }

    fn paint_child_for_root(&mut self, fragment: &Fragment) -> Option<published::PaintChild> {
        match fragment {
            Fragment::AbsoluteOrFixedPositioned(placement) => {
                Some(published::PaintChild::Placement(self.ensure_placement(placement)))
            }
            _ if fragment_out_of_flow_placement_id(fragment).is_some() => None,
            _ => self.paint_child_for_subtree(fragment),
        }
    }

    fn paint_child_for_subtree(&mut self, fragment: &Fragment) -> Option<published::PaintChild> {
        match fragment {
            Fragment::AbsoluteOrFixedPositioned(placement) => {
                Some(published::PaintChild::Placement(self.ensure_placement(placement)))
            }
            _ if fragment_out_of_flow_placement_id(fragment).is_some() => None,
            _ => internal_fragment_key(fragment)
                .and_then(|key| self.internal_to_node.get(&key).copied())
                .map(published::PaintChild::Fragment),
        }
    }

    fn ensure_placement(
        &mut self,
        placement: &OutOfFlowPlacementFragment,
    ) -> published::PlacementId {
        if let Some(id) = self.placement_ids.get(&placement.id).copied() {
            return id;
        }
        let fragment = self.placement_targets[&placement.id];
        let containing_block = self.nodes[fragment.0 as usize].parent;
        let id = published::PlacementId(self.placements.len() as u32);
        self.placements.push(published::OutOfFlowPlacement {
            fragment,
            containing_block,
            static_position_rect: placement.static_position_rect,
            resolved_alignment: havi_types::LogicalVec2 {
                inline: placement.resolved_alignment.inline,
                block: placement.resolved_alignment.block,
            },
            original_parent_writing_mode: placement.original_parent_writing_mode,
            position: placement.position,
        });
        self.placement_ids.insert(placement.id, id);
        id
    }
}

fn convert_specific_layout_info(
    info: Option<&SpecificLayoutInfo>,
) -> Option<published::SpecificLayoutInfo> {
    match info {
        Some(SpecificLayoutInfo::Grid(grid)) => Some(published::SpecificLayoutInfo::Grid(Box::new(
            published::GridLayoutInfo {
                rows: grid.rows.sizes.clone(),
                columns: grid.columns.sizes.clone(),
            },
        ))),
        Some(SpecificLayoutInfo::TableWrapper) => Some(published::SpecificLayoutInfo::TableWrapper),
        _ => None,
    }
}

fn convert_base(base: &super::BaseFragment) -> published::BaseFragment {
    published::BaseFragment {
        tag: base.tag.map(|tag| published::Tag {
            node: tag.node,
            pseudo: tag.pseudo_element_chain.primary,
        }),
        flags: published::FragmentFlags::from_bits_retain(base.flags.bits()),
        style: match base.style() {
            BaseFragmentStyleRef::Owned(style) => style.clone(),
            BaseFragmentStyleRef::Shared(style) => style.clone(),
        },
        rect: base.rect,
    }
}

fn convert_text_fragment(fragment: &TextFragment) -> published::TextFragment {
    let font_data_and_index = fragment.font.font_data_and_index().ok();
    let font_data = font_data_and_index
        .as_ref()
        .map(|data_and_index| Arc::new(data_and_index.data.as_ref().to_vec()));
    let glyphs = fragment
        .glyphs
        .iter()
        .flat_map(|glyph_store| glyph_store.glyphs())
        .map(|glyph| published::ShapedGlyph {
            glyph_id: glyph.id(),
            advance: {
                let mut advance = glyph.advance();
                if glyph.char_is_word_separator() {
                    advance += fragment.justification_adjustment;
                }
                advance
            },
            x_offset: glyph.offset().map_or(Au::new(0), |offset| offset.x),
            y_offset: glyph.offset().map_or(Au::new(0), |offset| offset.y),
            char_count: glyph.character_count() as u32,
        })
        .collect();
    published::TextFragment {
        base: convert_base(&fragment.base),
        text: fragment.text.clone(),
        font_size_px: fragment.font_metrics.em_size.to_f32_px(),
        glyphs,
        font_data,
        font_index: font_data_and_index.map(|data_and_index| data_and_index.index).unwrap_or(0),
        baseline_ascent: fragment.font_metrics.ascent,
        underline_offset: fragment.font_metrics.underline_offset,
        underline_size: fragment.font_metrics.underline_size,
        strikeout_offset: fragment.font_metrics.strikeout_offset,
        strikeout_size: fragment.font_metrics.strikeout_size,
        character_range_start: fragment
            .offsets
            .as_ref()
            .map(|offsets| offsets.character_range.start as u32)
            .unwrap_or(0),
    }
}

fn convert_image_fragment(fragment: &ImageFragment) -> published::ImageFragment {
    let (frame_width, frame_height, frame_byte_range, image_data) = match fragment.raster_image.as_ref() {
        Some(raster_image) => {
            let frame = raster_image.frames.first();
            (
                frame.map(|frame| frame.width).unwrap_or(raster_image.metadata.width),
                frame.map(|frame| frame.height).unwrap_or(raster_image.metadata.height),
                frame
                    .map(|frame| frame.byte_range.clone())
                    .unwrap_or(0..raster_image.bytes.len()),
                raster_image.bytes.clone(),
            )
        }
        None => {
            let image_data = fragment
                .source_data
                .clone()
                .unwrap_or_else(|| Arc::new(Vec::new()));
            (
                fragment.source_width,
                fragment.source_height,
                0..image_data.len(),
                image_data,
            )
        }
    };
    published::ImageFragment {
        base: convert_base(&fragment.base),
        image_key: fragment
            .image_key
            .map(|key| published::FragmentImageKey::from((key.0.0, key.1))),
        source_kind: match fragment.source_kind {
            crate::fragment_tree::ImageFragmentSourceKind::Raster => {
                published::ImageSourceKind::Raster
            }
            crate::fragment_tree::ImageFragmentSourceKind::Canvas => {
                published::ImageSourceKind::Canvas
            }
            crate::fragment_tree::ImageFragmentSourceKind::Video => {
                published::ImageSourceKind::Video
            }
        },
        image_revision: fragment.image_revision,
        frame_width,
        frame_height,
        image_data,
        frame_byte_range,
    }
}

fn resolve_background_images_for_base(
    base: &published::BaseFragment,
    image_resolver: &Arc<ImageResolver>,
    svg_text_layout_context: &SVGTextLayoutContext,
) -> Vec<Option<published::BackgroundImage>> {
    let background = base.style.get_background();
    let mut images = Vec::with_capacity(background.background_image.0.len());
    for (index, image) in background.background_image.0.iter().enumerate() {
        match image {
            style::values::computed::image::Image::Url(url_value) => {
                let Some(url) = url_value.url() else {
                    images.push(None);
                    continue;
                };
                let Ok(cached) = image_resolver.get_cached_image_for_url(
                    base.tag.map(|tag| tag.node).unwrap_or(style::dom::OpaqueNode(0)),
                    url.clone().into(),
                    layout_api::LayoutImageDestination::DisplayListBuilding,
                ) else {
                    images.push(None);
                    continue;
                };
                match cached {
                    net_traits::image_cache::Image::Raster(raster_image) => {
                        let (width, height, byte_range, data) = match raster_image.frames.first() {
                            Some(frame) => (
                                frame.width,
                                frame.height,
                                frame.byte_range.clone(),
                                raster_image.bytes.clone(),
                            ),
                            None => (
                                raster_image.metadata.width,
                                raster_image.metadata.height,
                                0..raster_image.bytes.len(),
                                raster_image.bytes.clone(),
                            ),
                        };
                        images.push(Some(published::BackgroundImage {
                            image_key: raster_image
                                .id
                                .map(|key| published::FragmentImageKey::from((key.0.0, key.1))),
                            source_kind: published::ImageSourceKind::Raster,
                            revision: 0,
                            width,
                            height,
                            data,
                            byte_range,
                            geometry: None,
                            svg_generation: None,
                        }));
                    }
                    net_traits::image_cache::Image::Vector(vector_image) => {
                        let Some(svg_bytes) = image_resolver.vector_image_bytes(vector_image.id) else {
                            images.push(None);
                            continue;
                        };
                        let intrinsic = extract_svg_root_metadata(&svg_bytes)
                            .ok()
                            .map(|metadata| compute_svg_image_intrinsic_sizes(&metadata.viewport))
                            .unwrap_or_default();
                        let Some(geometry) = resolve_background_layer_geometry(
                            base,
                            index,
                            intrinsic.width.unwrap_or(vector_image.metadata.width as f32),
                            intrinsic.height.unwrap_or(vector_image.metadata.height as f32),
                            intrinsic.ratio,
                        ) else {
                            images.push(None);
                            continue;
                        };
                        let Some(svg_generation) = build_native_background_svg_generation(
                            &svg_bytes,
                            &base.style,
                            svg_text_layout_context,
                            image_resolver,
                            geometry.tile_w,
                            geometry.tile_h,
                        ) else {
                            images.push(None);
                            continue;
                        };
                        images.push(Some(published::BackgroundImage {
                            image_key: None,
                            source_kind: published::ImageSourceKind::NativeSvg,
                            revision: vector_image.id.0,
                            width: vector_image.metadata.width,
                            height: vector_image.metadata.height,
                            data: Arc::new(Vec::new()),
                            byte_range: 0..0,
                            geometry: Some(geometry),
                            svg_generation: Some(Arc::new(svg_generation)),
                        }));
                    }
                }
            }
            _ => images.push(None),
        }
    }
    images
}

fn fragment_supplies_canvas_background(base: &published::BaseFragment) -> bool {
    let current_color = &base.style.get_inherited_text().color;
    let background_color = base
        .style
        .get_background()
        .background_color
        .resolve_to_absolute(current_color);
    if background_color.alpha > 0.001 {
        return true;
    }
    base.style
        .get_background()
        .background_image
        .0
        .iter()
        .any(|image| !matches!(image, Image::None))
}

fn root_allows_body_canvas_background(base: &published::BaseFragment) -> bool {
    let current_color = &base.style.get_inherited_text().color;
    let background = base.style.get_background();
    let background_color = background.background_color.resolve_to_absolute(current_color);
    background_color.alpha <= 0.001
        && background
            .background_image
            .0
            .iter()
            .all(|image| matches!(image, Image::None))
}

#[derive(Clone, Copy)]
struct BackgroundBoxInsets {
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
}

fn resolve_background_layer_geometry(
    base: &published::BaseFragment,
    layer_index: usize,
    natural_width: f32,
    natural_height: f32,
    natural_ratio: Option<f32>,
) -> Option<published::BackgroundLayerGeometry> {
    use style::computed_values::background_clip::single_value::T as Clip;
    use style::computed_values::background_origin::single_value::T as Origin;
    use style::values::computed::background::BackgroundSize;
    use style::values::specified::background::{
        BackgroundRepeat as RepeatXY, BackgroundRepeatKeyword as Repeat,
    };

    fn get_cyclic<T>(values: &[T], index: usize) -> &T {
        &values[index % values.len()]
    }

    fn sub_rect(
        x: f64,
        y: f64,
        w: f32,
        h: f32,
        border: &BackgroundBoxInsets,
        padding: &BackgroundBoxInsets,
        which: Origin,
    ) -> (f64, f64, f32, f32) {
        match which {
            Origin::BorderBox => (x, y, w, h),
            Origin::PaddingBox => (
                x + border.left as f64,
                y + border.top as f64,
                (w - border.left - border.right).max(0.0),
                (h - border.top - border.bottom).max(0.0),
            ),
            Origin::ContentBox => (
                x + (border.left + padding.left) as f64,
                y + (border.top + padding.top) as f64,
                (w - border.left - border.right - padding.left - padding.right).max(0.0),
                (h - border.top - border.bottom - padding.top - padding.bottom).max(0.0),
            ),
        }
    }

    fn clip_to_origin(clip: Clip) -> Origin {
        match clip {
            Clip::BorderBox => Origin::BorderBox,
            Clip::PaddingBox => Origin::PaddingBox,
            Clip::ContentBox => Origin::ContentBox,
        }
    }

    fn layout_1d(
        tile_size: &mut f32,
        mut repeat: Repeat,
        position: &style::values::computed::LengthPercentage,
        painting_area_origin: f32,
        painting_area_size: f32,
        positioning_area_size: f32,
    ) -> (f32, f32) {
        if let Repeat::Round = repeat {
            if positioning_area_size > 0.0 {
                *tile_size = positioning_area_size / (positioning_area_size / *tile_size).round().max(1.0);
            }
        }

        let mut origin = position
            .to_used_value(app_units::Au::from_f32_px(positioning_area_size - *tile_size))
            .to_f32_px();
        let mut spacing = 0.0;
        if let Repeat::Space = repeat {
            let count = (positioning_area_size / *tile_size).floor();
            if count >= 2.0 {
                origin = 0.0;
                spacing = (positioning_area_size - *tile_size * count) / (count - 1.0);
            } else {
                repeat = Repeat::NoRepeat;
            }
        }

        match repeat {
            Repeat::Repeat | Repeat::Round | Repeat::Space => {
                let stride = *tile_size + spacing;
                let offset = origin - painting_area_origin;
                let origin = origin - stride * (offset / stride).ceil();
                let end = painting_area_origin + painting_area_size;
                (origin, end - origin)
            }
            Repeat::NoRepeat => (origin, *tile_size),
        }
    }

    use style::values::specified::border::BorderStyle;

    let computed = &base.style;
    let border = computed.get_border();
    let border_width = |style: BorderStyle, width: style::values::computed::BorderSideWidth| -> f32 {
        if matches!(style, BorderStyle::None | BorderStyle::Hidden) {
            0.0
        } else {
            width.0.to_f32_px().max(0.0)
        }
    };
    let border_insets = BackgroundBoxInsets {
        top: border_width(border.clone_border_top_style(), border.clone_border_top_width()),
        right: border_width(border.clone_border_right_style(), border.clone_border_right_width()),
        bottom: border_width(border.clone_border_bottom_style(), border.clone_border_bottom_width()),
        left: border_width(border.clone_border_left_style(), border.clone_border_left_width()),
    };

    let padding = computed.get_padding();
    let padding_insets = BackgroundBoxInsets {
        top: padding.padding_top.0.to_length().map_or(0.0, |length| length.px()),
        right: padding.padding_right.0.to_length().map_or(0.0, |length| length.px()),
        bottom: padding.padding_bottom.0.to_length().map_or(0.0, |length| length.px()),
        left: padding.padding_left.0.to_length().map_or(0.0, |length| length.px()),
    };

    let background = computed.get_background();
    let x = base.rect.origin.x.to_f32_px() as f64;
    let y = base.rect.origin.y.to_f32_px() as f64;
    let w = base.rect.size.width.to_f32_px();
    let h = base.rect.size.height.to_f32_px();

    let origin = *get_cyclic(&background.background_origin.0, layer_index);
    let clip = *get_cyclic(&background.background_clip.0, layer_index);
    let (position_x, position_y, position_w, position_h) =
        sub_rect(x, y, w, h, &border_insets, &padding_insets, origin);
    let (paint_x, paint_y, paint_w, paint_h) = sub_rect(
        x,
        y,
        w,
        h,
        &border_insets,
        &padding_insets,
        clip_to_origin(clip),
    );

    let natural_ratio = if natural_width > 0.0 && natural_height > 0.0 {
        Some(natural_width / natural_height)
    } else {
        natural_ratio.filter(|ratio| *ratio > 0.0)
    };

    let mut tile_w;
    let mut tile_h;
    match get_cyclic(&background.background_size.0, layer_index) {
        BackgroundSize::Contain | BackgroundSize::Cover => {
            tile_w = position_w;
            tile_h = position_h;
            if let Some(natural_ratio) = natural_ratio {
                let position_ratio = position_w / position_h;
                let fit_width = match get_cyclic(&background.background_size.0, layer_index) {
                    BackgroundSize::Contain => position_ratio <= natural_ratio,
                    BackgroundSize::Cover => position_ratio > natural_ratio,
                    BackgroundSize::ExplicitSize { .. } => unreachable!(),
                };
                if fit_width {
                    tile_h = tile_w / natural_ratio;
                } else {
                    tile_w = tile_h * natural_ratio;
                }
            }
        }
        BackgroundSize::ExplicitSize { width, height } => {
            let mut explicit_w = width.non_auto().map(|value| {
                value.0.to_used_value(app_units::Au::from_f32_px(position_w)).to_f32_px()
            });
            let mut explicit_h = height.non_auto().map(|value| {
                value.0.to_used_value(app_units::Au::from_f32_px(position_h)).to_f32_px()
            });
            if explicit_w.is_none() && explicit_h.is_none() {
                explicit_w = Some(natural_width);
                explicit_h = Some(natural_height);
            }
            match (explicit_w, explicit_h) {
                (Some(tile_width), Some(tile_height)) => {
                    tile_w = tile_width;
                    tile_h = tile_height;
                }
                (Some(tile_width), None) => {
                    tile_w = tile_width;
                    tile_h = natural_ratio.map(|ratio| tile_width / ratio).unwrap_or(position_h);
                }
                (None, Some(tile_height)) => {
                    tile_h = tile_height;
                    tile_w = natural_ratio.map(|ratio| tile_height * ratio).unwrap_or(position_w);
                }
                (None, None) => {
                    tile_w = position_w;
                    tile_h = position_h;
                }
            }
        }
    }

    if tile_w <= 0.0 || tile_h <= 0.0 {
        return None;
    }

    let RepeatXY(repeat_x, repeat_y) = *get_cyclic(&background.background_repeat.0, layer_index);
    let (layout_x_origin, layout_x_size) = layout_1d(
        &mut tile_w,
        repeat_x,
        get_cyclic(&background.background_position_x.0, layer_index),
        paint_x as f32 - position_x as f32,
        paint_w,
        position_w,
    );
    let (layout_y_origin, layout_y_size) = layout_1d(
        &mut tile_h,
        repeat_y,
        get_cyclic(&background.background_position_y.0, layer_index),
        paint_y as f32 - position_y as f32,
        paint_h,
        position_h,
    );

    Some(published::BackgroundLayerGeometry {
        bounds_x: position_x + layout_x_origin as f64,
        bounds_y: position_y + layout_y_origin as f64,
        bounds_w: layout_x_size,
        bounds_h: layout_y_size,
        tile_w,
        tile_h,
        paint_x,
        paint_y,
        paint_w,
        paint_h,
    })
}

fn build_native_background_svg_generation(
    svg_bytes: &[u8],
    default_style: &servo_arc::Arc<style::properties::ComputedValues>,
    svg_text_layout_context: &SVGTextLayoutContext,
    image_resolver: &Arc<ImageResolver>,
    tile_width: f32,
    tile_height: f32,
) -> Option<published::FragmentArenaGeneration> {
    let svg_tree = parse_svg_tree(svg_bytes).ok()?;
    let rect = PhysicalRect::new(
        PhysicalPoint::origin(),
        PhysicalSize::new(Au::from_f32_px(tile_width), Au::from_f32_px(tile_height)),
    );
    let mut positioning_context = PositioningContext::default();
    let root = build_svg_root_fragment_from_tree_with_text_context(
        &svg_tree,
        default_style.clone(),
        svg_text_layout_context,
        &mut positioning_context,
        BaseFragmentInfo {
            tag: None,
            flags: FragmentFlags::empty(),
        },
        default_style,
        rect,
    )?;
    let root_fragments = vec![root];
    let containing_blocks = collect_containing_blocks(&root_fragments, &rect);
    let scrollable_overflow = compute_tree_scrollable_overflow(&root_fragments, rect);
    Some(build_generation(
        &root_fragments,
        &containing_blocks,
        rect,
        scrollable_overflow,
        image_resolver,
        svg_text_layout_context,
    ))
}

fn collect_containing_blocks(
    root_fragments: &[Fragment],
    initial_containing_block: &PhysicalRect<Au>,
) -> HashMap<usize, PhysicalRect<Au>> {
    let mut result = HashMap::new();
    let manager = ContainingBlockManager {
        for_non_absolute_descendants: initial_containing_block,
        for_absolute_descendants: None,
        for_absolute_and_fixed_descendants: initial_containing_block,
    };
    for fragment in root_fragments {
        fragment.find(&manager, 0, &mut |fragment, _level, containing_block| {
            if let Some(key) = internal_fragment_key(fragment) {
                result.insert(key, *containing_block);
            }
            None::<()>
        });
    }
    result
}

fn compute_tree_scrollable_overflow(
    root_fragments: &[Fragment],
    initial_containing_block: PhysicalRect<Au>,
) -> PhysicalRect<Au> {
    let Some(first_root_fragment) = root_fragments.first() else {
        return initial_containing_block;
    };

    let scrollable_overflow = root_fragments.iter().fold(initial_containing_block, |overflow, fragment| {
        let overflow_from_fragment = fragment.calculate_scrollable_overflow_for_parent();
        if fragment
            .retrieve_box_fragment()
            .is_some_and(|box_fragment| box_fragment.borrow().style().get_box().position == Position::Fixed)
        {
            return overflow;
        }
        overflow.union(&overflow_from_fragment)
    });

    let first_root_fragment = match first_root_fragment {
        Fragment::Box(fragment) | Fragment::Float(fragment) => fragment.borrow(),
        _ => return scrollable_overflow,
    };
    if !first_root_fragment.is_root_element() {
        return scrollable_overflow;
    }
    first_root_fragment.clip_wholly_unreachable_scrollable_overflow(
        scrollable_overflow,
        initial_containing_block,
    )
}

fn print_generation_fragment(
    generation: &published::FragmentArenaGeneration,
    id: published::FragmentId,
    tree: &mut PrintTree,
) {
    let node = generation.node(id);
    tree.new_level(format!("{:?} {:?}", id, node.base().rect));
    for child in generation.geometry_children(id) {
        print_generation_fragment(generation, *child, tree);
    }
    tree.end_level();
}

fn convert_fragment_base(fragment: &Fragment) -> published::BaseFragment {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => convert_base(&bf.borrow().base),
        Fragment::Positioning(pf) => convert_base(&pf.borrow().base),
        Fragment::Text(tf) => convert_base(&tf.borrow().base),
        Fragment::Image(imf) => convert_base(&imf.borrow().base),
        Fragment::IFrame(ifr) => convert_base(&ifr.borrow().base),
        Fragment::SVGViewport(fragment) => convert_base(&fragment.borrow().base),
        Fragment::SVGContainer(fragment) => convert_base(&fragment.borrow().base),
        Fragment::SVGLeaf(fragment) => convert_base(&fragment.borrow().base),
        Fragment::AbsoluteOrFixedPositioned(_) => {
            unreachable!("filtered by internal_fragment_key")
        }
    }
}

fn published_fragment_mapping_tag(
    fragment: &Fragment,
    base: &published::BaseFragment,
) -> Option<published::Tag> {
    match fragment {
        Fragment::SVGViewport(fragment) => Some(
            fragment
                .borrow()
                .identity
                .current_instance_owner_or_source_tag(),
        ),
        Fragment::SVGContainer(fragment) => Some(
            fragment
                .borrow()
                .identity
                .current_instance_owner_or_source_tag(),
        ),
        Fragment::SVGLeaf(fragment) => Some(
            fragment
                .borrow()
                .identity
                .current_instance_owner_or_source_tag(),
        ),
        _ => base.tag,
    }
}

fn fragment_out_of_flow_placement_id(fragment: &Fragment) -> Option<u32> {
    match fragment {
        Fragment::Box(fragment) | Fragment::Float(fragment) => fragment.borrow().base.out_of_flow_placement_id,
        Fragment::Positioning(fragment) => fragment.borrow().base.out_of_flow_placement_id,
        Fragment::Text(fragment) => fragment.borrow().base.out_of_flow_placement_id,
        Fragment::Image(fragment) => fragment.borrow().base.out_of_flow_placement_id,
        Fragment::IFrame(fragment) => fragment.borrow().base.out_of_flow_placement_id,
        Fragment::SVGViewport(fragment) => fragment.borrow().base.out_of_flow_placement_id,
        Fragment::SVGContainer(fragment) => fragment.borrow().base.out_of_flow_placement_id,
        Fragment::SVGLeaf(fragment) => fragment.borrow().base.out_of_flow_placement_id,
        Fragment::AbsoluteOrFixedPositioned(_) => None,
    }
}

fn internal_fragment_key(fragment: &Fragment) -> Option<usize> {
    match fragment {
        Fragment::Box(fragment) => Some(fragment_key(fragment)),
        Fragment::Float(fragment) => Some(fragment_key(fragment)),
        Fragment::Positioning(fragment) => Some(fragment_key(fragment)),
        Fragment::Text(fragment) => Some(fragment_key(fragment)),
        Fragment::Image(fragment) => Some(fragment_key(fragment)),
        Fragment::IFrame(fragment) => Some(fragment_key(fragment)),
        Fragment::SVGViewport(fragment) => Some(fragment_key(fragment)),
        Fragment::SVGContainer(fragment) => Some(fragment_key(fragment)),
        Fragment::SVGLeaf(fragment) => Some(fragment_key(fragment)),
        Fragment::AbsoluteOrFixedPositioned(_) => None,
    }
}

fn fragment_key<T>(fragment: &crate::cell::ArcRefCell<T>) -> usize {
    (&**fragment as *const atomic_refcell::AtomicRefCell<T>) as usize
}

fn convert_collapsed_margin(margin: super::CollapsedMargin) -> published::CollapsedMargin {
    published::CollapsedMargin::new(margin.solve())
}

fn remap_svg_resource_node(
    mut node: published::SVGResourceNode,
    resource_offset: u32,
) -> published::SVGResourceNode {
    match &mut node.kind {
        published::SVGResourceKind::PaintServer(published::SVGPaintServerResource::Pattern(resource)) => {
            resource.source_resource_dependencies = resource
                .source_resource_dependencies
                .iter()
                .copied()
                .map(|id| remap_svg_resource_id(id, Some(resource_offset)))
                .collect();
        }
        published::SVGResourceKind::UseInstanceSource(resource) => {
            resource.source_resource_dependencies = resource
                .source_resource_dependencies
                .iter()
                .copied()
                .map(|id| remap_svg_resource_id(id, Some(resource_offset)))
                .collect();
        }
        _ => {}
    }
    node
}

fn remap_svg_effect_state(
    effects: published::SVGEffectState,
    resource_offset: Option<u32>,
) -> published::SVGEffectState {
    published::SVGEffectState {
        clip_path: effects.clip_path.map(|id| remap_svg_resource_id(id, resource_offset)),
        mask: effects.mask.map(|id| remap_svg_resource_id(id, resource_offset)),
        filter: effects.filter.map(|id| remap_svg_resource_id(id, resource_offset)),
        marker_start: effects
            .marker_start
            .map(|id| remap_svg_resource_id(id, resource_offset)),
        marker_mid: effects
            .marker_mid
            .map(|id| remap_svg_resource_id(id, resource_offset)),
        marker_end: effects
            .marker_end
            .map(|id| remap_svg_resource_id(id, resource_offset)),
    }
}

fn remap_svg_paint(paint: published::SVGPaint, resource_offset: Option<u32>) -> published::SVGPaint {
    match paint {
        published::SVGPaint::Server(id) => {
            published::SVGPaint::Server(remap_svg_resource_id(id, resource_offset))
        }
        _ => paint,
    }
}

fn remap_svg_stroke_style(
    mut stroke: published::SVGStrokeStyle,
    resource_offset: Option<u32>,
) -> published::SVGStrokeStyle {
    stroke.paint = remap_svg_paint(stroke.paint, resource_offset);
    stroke
}

fn remap_svg_paint_style(
    mut paint: published::SVGPaintStyle,
    resource_offset: Option<u32>,
) -> published::SVGPaintStyle {
    paint.fill = remap_svg_paint(paint.fill, resource_offset);
    paint.stroke = paint
        .stroke
        .map(|stroke| remap_svg_stroke_style(stroke, resource_offset));
    paint
}

fn remap_svg_leaf_kind(
    mut kind: published::SVGLeafKind,
    _resource_offset: Option<u32>,
) -> published::SVGLeafKind {
    match &mut kind {
        published::SVGLeafKind::Path(_) => {}
        published::SVGLeafKind::Text(_) => {}
        published::SVGLeafKind::Image(_) => {}
    }
    if let published::SVGLeafKind::Path(_) = &kind {
        return kind;
    }
    kind
}

fn remap_svg_bounds(
    bounds: published::SVGBounds,
    _resource_offset: Option<u32>,
) -> published::SVGBounds {
    bounds
}

fn remap_svg_resource_id(
    id: published::SVGResourceId,
    resource_offset: Option<u32>,
) -> published::SVGResourceId {
    published::SVGResourceId(id.0 + resource_offset.unwrap_or(0))
}

