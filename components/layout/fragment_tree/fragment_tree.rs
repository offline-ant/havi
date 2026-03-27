/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use app_units::Au;
use base::print_tree::PrintTree;
use bitflags::bitflags;
use havi_types::fragment_tree as published;
use rustc_hash::FxHashSet;
use style::animation::AnimationSetKey;
use style::computed_values::position::T as Position;
use style::values::specified::Overflow;

use super::{
    BaseFragmentStyleRef, ContainingBlockManager, Fragment, ImageFragment,
    OutOfFlowPlacementFragment, SpecificLayoutInfo, TextFragment,
};
use crate::context::{ImageResolver, LayoutContext};
use crate::geom::PhysicalRect;

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

        let generation = Arc::new(build_generation(
            root_fragments.as_ref(),
            &containing_blocks,
            initial_containing_block,
            scrollable_overflow,
            &layout_context.image_resolver,
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
            .map(|node| resolve_background_images_for_base(node.base(), image_resolver))
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
            }),
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
) -> published::FragmentArenaGeneration {
    let mut builder = ArenaBuilder::new(containing_blocks, image_resolver);
    let geometry_roots: Vec<_> = root_fragments
        .iter()
        .filter_map(|fragment| builder.build_geometry(fragment, None))
        .collect();
    builder.populate_paint_children(root_fragments);

    let paint_roots = root_fragments
        .iter()
        .filter_map(|fragment| builder.paint_child_for_root(fragment))
        .collect::<Vec<_>>();

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
        svg_resources: Arc::from(Vec::<published::SVGResourceNode>::new()),
        initial_containing_block,
        scrollable_overflow,
    }
}

struct ArenaBuilder<'a> {
    containing_blocks: &'a HashMap<usize, PhysicalRect<Au>>,
    image_resolver: &'a Arc<ImageResolver>,
    nodes: Vec<published::FragmentNode>,
    placements: Vec<published::OutOfFlowPlacement>,
    derived: published::FragmentDerivedData,
    internal_to_node: HashMap<usize, published::FragmentId>,
    node_fragments: HashMap<published::FragmentMapKey, Vec<published::FragmentId>>,
    placement_targets: HashMap<u32, published::FragmentId>,
    placement_ids: HashMap<u32, published::PlacementId>,
}

impl<'a> ArenaBuilder<'a> {
    fn new(
        containing_blocks: &'a HashMap<usize, PhysicalRect<Au>>,
        image_resolver: &'a Arc<ImageResolver>,
    ) -> Self {
        Self {
            containing_blocks,
            image_resolver,
            nodes: Vec::new(),
            placements: Vec::new(),
            derived: published::FragmentDerivedData {
                containing_blocks: Vec::new(),
                scrollable_overflow: Vec::new(),
                sticky_insets: Vec::new(),
                background_images: Vec::new(),
            },
            internal_to_node: HashMap::new(),
            node_fragments: HashMap::new(),
            placement_targets: HashMap::new(),
            placement_ids: HashMap::new(),
        }
    }

    fn build_geometry(
        &mut self,
        fragment: &Fragment,
        parent: Option<published::FragmentId>,
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
        self.record_node_mapping(id, base.tag);
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
                    .filter_map(|child| self.build_geometry(child, Some(id)))
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
                    .filter_map(|child| self.build_geometry(child, Some(id)))
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
                let geometry_children = svg_fragment
                    .children
                    .iter()
                    .filter_map(|child| self.build_geometry(child, Some(id)))
                    .collect();
                published::FragmentKind::SVGViewport(published::SVGViewportFragment {
                    base,
                    geometry_children,
                    paint_children: Vec::new(),
                    viewport_rect: svg_fragment.viewport_rect,
                    view_box_rect: svg_fragment.view_box_rect,
                    local_to_parent_transform: svg_fragment.local_to_parent_transform,
                    overflow_clip: svg_fragment.overflow_clip.clone(),
                })
            }
            Fragment::SVGGroup(svg_fragment) => {
                let svg_fragment = svg_fragment.borrow();
                let geometry_children = svg_fragment
                    .children
                    .iter()
                    .filter_map(|child| self.build_geometry(child, Some(id)))
                    .collect();
                published::FragmentKind::SVGGroup(published::SVGGroupFragment {
                    base,
                    geometry_children,
                    paint_children: Vec::new(),
                    local_transform: svg_fragment.local_transform,
                    opacity: svg_fragment.opacity,
                    resources: svg_fragment.resources.clone(),
                })
            }
            Fragment::SVGPath(svg_fragment) => {
                let svg_fragment = svg_fragment.borrow();
                published::FragmentKind::SVGPath(published::SVGPathFragment {
                    base,
                    path: svg_fragment.path.clone(),
                    object_bounding_box: svg_fragment.object_bounding_box,
                    decorated_bounding_box: svg_fragment.decorated_bounding_box,
                    local_transform: svg_fragment.local_transform,
                    fill: svg_fragment.fill.clone(),
                    stroke: svg_fragment.stroke.clone(),
                    resources: svg_fragment.resources.clone(),
                })
            }
            Fragment::SVGText(svg_fragment) => {
                let svg_fragment = svg_fragment.borrow();
                published::FragmentKind::SVGText(published::SVGTextFragment {
                    base,
                    glyph_runs: svg_fragment.glyph_runs.clone(),
                    object_bounding_box: svg_fragment.object_bounding_box,
                    decorated_bounding_box: svg_fragment.decorated_bounding_box,
                    local_transform: svg_fragment.local_transform,
                    resources: svg_fragment.resources.clone(),
                })
            }
            Fragment::SVGForeignObject(svg_fragment) => {
                let svg_fragment = svg_fragment.borrow();
                let geometry_children = svg_fragment
                    .children
                    .iter()
                    .filter_map(|child| self.build_geometry(child, Some(id)))
                    .collect();
                published::FragmentKind::SVGForeignObject(published::SVGForeignObjectFragment {
                    base,
                    geometry_children,
                    paint_children: Vec::new(),
                    svg_viewport_rect: svg_fragment.svg_viewport_rect,
                    local_transform: svg_fragment.local_transform,
                })
            }
            Fragment::SVGImage(svg_fragment) => {
                let svg_fragment = svg_fragment.borrow();
                published::FragmentKind::SVGImage(published::SVGImageFragment {
                    base,
                    viewport_rect: svg_fragment.viewport_rect,
                    local_transform: svg_fragment.local_transform,
                    href: svg_fragment.href.clone(),
                    resources: svg_fragment.resources.clone(),
                })
            }
            Fragment::AbsoluteOrFixedPositioned(_) => unreachable!("filtered by internal_fragment_key"),
        };

        self.nodes[id.0 as usize] = published::FragmentNode { parent, kind };
        Some(id)
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
            Fragment::SVGGroup(_) |
            Fragment::SVGPath(_) |
            Fragment::SVGText(_) |
            Fragment::SVGForeignObject(_) |
            Fragment::SVGImage(_) => base.rect,
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
        self.derived.background_images.push(resolve_background_images_for_base(base, self.image_resolver));
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
            Fragment::SVGGroup(svg_fragment) => svg_fragment.borrow().children.clone(),
            Fragment::SVGForeignObject(svg_fragment) => svg_fragment.borrow().children.clone(),
            Fragment::Text(_) |
            Fragment::Image(_) |
            Fragment::IFrame(_) |
            Fragment::SVGPath(_) |
            Fragment::SVGText(_) |
            Fragment::SVGImage(_) => Vec::new(),
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
            published::FragmentKind::SVGGroup(svg_fragment) => {
                svg_fragment.paint_children = paint_children;
            }
            published::FragmentKind::SVGForeignObject(svg_fragment) => {
                svg_fragment.paint_children = paint_children;
            }
            published::FragmentKind::Text(_) |
            published::FragmentKind::Image(_) |
            published::FragmentKind::IFrame(_) |
            published::FragmentKind::SVGPath(_) |
            published::FragmentKind::SVGText(_) |
            published::FragmentKind::SVGImage(_) => {}
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
        None => (0, 0, 0..0, Arc::new(Vec::new())),
    };
    published::ImageFragment {
        base: convert_base(&fragment.base),
        image_key: fragment.image_key.as_ref().map(hash_value),
        frame_width,
        frame_height,
        image_data,
        frame_byte_range,
    }
}

fn resolve_background_images_for_base(
    base: &published::BaseFragment,
    image_resolver: &Arc<ImageResolver>,
) -> Vec<Option<published::BackgroundImage>> {
    let background = base.style.get_background();
    let mut images = Vec::with_capacity(background.background_image.0.len());
    for image in background.background_image.0.iter() {
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
                let Some(raster_image) = cached.as_raster_image() else {
                    images.push(None);
                    continue;
                };
                let (width, height, bytes) = match raster_image.frames.first() {
                    Some(frame) => (
                        frame.width,
                        frame.height,
                        raster_image.bytes[frame.byte_range.clone()].to_vec(),
                    ),
                    None => (
                        raster_image.metadata.width,
                        raster_image.metadata.height,
                        raster_image.bytes.as_ref().clone(),
                    ),
                };
                images.push(Some(published::BackgroundImage {
                    width,
                    height,
                    pixels: bytes,
                }));
            }
            _ => images.push(None),
        }
    }
    images
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
        Fragment::SVGGroup(fragment) => convert_base(&fragment.borrow().base),
        Fragment::SVGPath(fragment) => convert_base(&fragment.borrow().base),
        Fragment::SVGText(fragment) => convert_base(&fragment.borrow().base),
        Fragment::SVGForeignObject(fragment) => convert_base(&fragment.borrow().base),
        Fragment::SVGImage(fragment) => convert_base(&fragment.borrow().base),
        Fragment::AbsoluteOrFixedPositioned(_) => {
            unreachable!("filtered by internal_fragment_key")
        }
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
        Fragment::SVGGroup(fragment) => fragment.borrow().base.out_of_flow_placement_id,
        Fragment::SVGPath(fragment) => fragment.borrow().base.out_of_flow_placement_id,
        Fragment::SVGText(fragment) => fragment.borrow().base.out_of_flow_placement_id,
        Fragment::SVGForeignObject(fragment) => fragment.borrow().base.out_of_flow_placement_id,
        Fragment::SVGImage(fragment) => fragment.borrow().base.out_of_flow_placement_id,
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
        Fragment::SVGGroup(fragment) => Some(fragment_key(fragment)),
        Fragment::SVGPath(fragment) => Some(fragment_key(fragment)),
        Fragment::SVGText(fragment) => Some(fragment_key(fragment)),
        Fragment::SVGForeignObject(fragment) => Some(fragment_key(fragment)),
        Fragment::SVGImage(fragment) => Some(fragment_key(fragment)),
        Fragment::AbsoluteOrFixedPositioned(_) => None,
    }
}

fn fragment_key<T>(fragment: &crate::cell::ArcRefCell<T>) -> usize {
    (&**fragment as *const atomic_refcell::AtomicRefCell<T>) as usize
}

fn convert_collapsed_margin(margin: super::CollapsedMargin) -> published::CollapsedMargin {
    published::CollapsedMargin::new(margin.solve())
}

fn hash_value<T: Hash>(value: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

