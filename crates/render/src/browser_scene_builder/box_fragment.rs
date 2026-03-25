use havi_types::fragment_tree as published;
use makepad_browser_scene::{
    MpClipChain, MpClipKind, MpClipNode, MpPerCornerRadius, MpReferenceFrame, MpScene,
    MpScrollFrame, MpSpatialKind, MpSpatialNode, MpStickyFrame, MpStickyOffsets, ResourceRegistry,
};
use makepad_widgets::{dvec2, Cx2d, DVec2, Rect};
use style::computed_values::overflow_x::T as ComputedOverflow;
use style::values::computed::ClipRectOrAuto;

use super::effects::lower_box_effect_node;
use super::geometry::{map_box_rect_to_spatial_space, physical_rect_to_rect};
use super::traversal::{build_paint_list, owner_node_id_for_fragment, push_fragment_primitives};
use super::{BuildContext, BuildState, BrowserDocumentScrollNodes, DirectBuilderIds};
use crate::background::resolve_border_radii;
use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::RenderPaintItem;
use crate::reference_frame::reference_frame_semantics;

pub(super) fn build_box_fragment(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
    bf: &published::BoxFragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    scroll_nodes: &mut BrowserDocumentScrollNodes,
    previous_document: Option<&makepad_browser_scene::MpDocument>,
) -> Result<(), String> {
    let skip_own_paint = bf.base.flags.intersects(published::FragmentFlags::DO_NOT_PAINT);

    let border_rect = physical_rect_to_rect(bf.border_rect());
    let content_rect = physical_rect_to_rect(bf.content_rect());
    let box_origin_in_parent = build_cx.containing_block_origin + border_rect.pos;

    let mut box_cx = build_cx;
    let mut uses_box_local_basis = false;
    if let Some(semantics) = reference_frame_semantics(bf, box_origin_in_parent) {
        box_cx.spatial_id = scene.push_spatial_node(MpSpatialNode {
            parent: Some(box_cx.spatial_id),
            kind: MpSpatialKind::ReferenceFrame(MpReferenceFrame {
                viewport_rect: Rect {
                    pos: dvec2(0.0, 0.0),
                    size: border_rect.size,
                },
                placement_origin: semantics.placement_origin,
                transform: semantics.transform_matrix,
                perspective: semantics.perspective_matrix,
                transform_style: semantics.transform_style,
                backface_visibility: semantics.backface_visibility,
                flattens_descendants: semantics.flattens_descendants,
            }),
        });
        uses_box_local_basis = true;
    }

    if let Some(sticky_frame) = sticky_frame_for_box(
        generation,
        fragment_id,
        bf,
        box_origin_in_parent,
        border_rect.size,
        uses_box_local_basis,
    ) {
        box_cx.spatial_id = scene.push_spatial_node(MpSpatialNode {
            parent: Some(box_cx.spatial_id),
            kind: MpSpatialKind::StickyFrame(sticky_frame),
        });
        uses_box_local_basis = true;
    }

    if let Some(rect) = css_clip_rect(bf) {
        box_cx.clip_chain_id = push_clip_chain(
            scene,
            box_cx.clip_chain_id,
            box_cx.spatial_id,
            MpClipKind::Rect {
                rect: map_box_rect_to_spatial_space(
                    rect,
                    build_cx.containing_block_origin,
                    border_rect.pos,
                    uses_box_local_basis,
                ),
            },
        );
    }

    let effect = match lower_box_effect_node(bf, box_cx.spatial_id, box_cx.clip_chain_id) {
        Ok(effect) => effect,
        Err(reason) => {
            super::log_builder_skip_once(reason);
            return Ok(());
        }
    };
    if let Some(effect) = effect {
        box_cx.effect_id = Some(scene.push_effect(effect));
    }

    let item_origin = if uses_box_local_basis {
        dvec2(-border_rect.pos.x, -border_rect.pos.y)
    } else {
        build_cx.containing_block_origin
    };
    if !skip_own_paint {
        push_fragment_primitives(
            cx,
            generation,
            scene,
            registry,
            state,
            &RenderPaintItem {
                section: StackingContextSection::OwnBackgroundsAndBorders,
                local_origin: item_origin,
                fragment_id,
            },
            owner_node_id_for_fragment(generation, fragment_id),
            box_cx,
        )?;
    }

    let mut child_cx = box_cx;
    if needs_overflow_clip(bf) {
        let radius = resolve_border_radii(&bf.base.style).max();
        let padding_rect = physical_rect_to_rect(bf.padding_rect());
        let rect = map_box_rect_to_spatial_space(
            padding_rect,
            build_cx.containing_block_origin,
            border_rect.pos,
            uses_box_local_basis,
        );
        child_cx.clip_chain_id = push_clip_chain(
            scene,
            child_cx.clip_chain_id,
            box_cx.spatial_id,
            if radius > 0.0 {
                MpClipKind::RoundedRect {
                    rect,
                    radius: MpPerCornerRadius::uniform(radius),
                }
            } else {
                MpClipKind::Rect { rect }
            },
        );
    }

    if let Some(scroll_offset) = scroll_offset_for_box(bf, scroll_state) {
        child_cx.spatial_id = scene.push_spatial_node(MpSpatialNode {
            parent: Some(box_cx.spatial_id),
            kind: MpSpatialKind::ScrollFrame(MpScrollFrame {
                viewport_rect: Rect {
                    pos: if uses_box_local_basis {
                        dvec2(0.0, 0.0)
                    } else {
                        box_origin_in_parent
                    },
                    size: border_rect.size,
                },
                content_rect: Rect {
                    pos: dvec2(0.0, 0.0),
                    size: physical_rect_to_rect(generation.scrollable_overflow_for(fragment_id)).size,
                },
                scroll_offset,
            }),
        });
        if let Some(node_id) = bf.base.tag.map(|tag| tag.node.0) {
            scroll_nodes.spatial_nodes.insert(node_id, child_cx.spatial_id);
        }
        uses_box_local_basis = true;
    }

    child_cx.containing_block_origin = if uses_box_local_basis {
        content_rect.pos - border_rect.pos
    } else {
        build_cx.containing_block_origin + content_rect.pos
    };
    build_paint_list(
        cx,
        generation,
        &bf.paint_children,
        scroll_state,
        scene,
        registry,
        state,
        ids,
        child_cx,
        scroll_nodes,
        previous_document,
    )
}

fn scroll_offset_for_box(
    bf: &published::BoxFragment,
    scroll_state: &crate::ScrollState,
) -> Option<DVec2> {
    if !needs_overflow_clip(bf) {
        return None;
    }
    Some(
        bf.base
            .tag
            .and_then(|tag| scroll_state.get(&tag.node.0).copied())
            .unwrap_or_else(|| dvec2(0.0, 0.0)),
    )
}

fn sticky_frame_for_box(
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
    bf: &published::BoxFragment,
    box_origin_in_parent: DVec2,
    box_size: DVec2,
    uses_box_local_basis: bool,
) -> Option<MpStickyFrame> {
    if bf.base.style.get_box().position != style::computed_values::position::T::Sticky {
        return None;
    }
    let insets = generation.sticky_insets_for(fragment_id)?;
    let border_rect = physical_rect_to_rect(bf.border_rect());
    let containing_block_rect = physical_rect_to_rect(generation.containing_block(fragment_id));
    let map_auto_or_length = |value: &havi_types::geom::AuOrAuto| match value {
        style::values::generics::length::GenericLengthPercentageOrAuto::Auto => None,
        style::values::generics::length::GenericLengthPercentageOrAuto::LengthPercentage(value) => {
            Some(value.to_f32_px())
        }
    };
    let frame_rect = if uses_box_local_basis {
        Rect {
            pos: dvec2(0.0, 0.0),
            size: box_size,
        }
    } else {
        Rect {
            pos: box_origin_in_parent,
            size: box_size,
        }
    };
    let containing_block_rect = if uses_box_local_basis {
        Rect {
            pos: containing_block_rect.pos - border_rect.pos,
            size: containing_block_rect.size,
        }
    } else {
        containing_block_rect
    };
    Some(MpStickyFrame {
        frame_rect,
        containing_block_rect,
        margins: MpStickyOffsets {
            top: map_auto_or_length(&insets.top),
            right: map_auto_or_length(&insets.right),
            bottom: map_auto_or_length(&insets.bottom),
            left: map_auto_or_length(&insets.left),
        },
    })
}

fn needs_overflow_clip(bf: &published::BoxFragment) -> bool {
    let overflow = bf.base.style.get_box();
    !matches!(overflow.overflow_x, ComputedOverflow::Visible)
        || !matches!(overflow.overflow_y, ComputedOverflow::Visible)
}

fn css_clip_rect(bf: &published::BoxFragment) -> Option<Rect> {
    if !bf.base.style.get_box().position.is_absolutely_positioned() {
        return None;
    }
    let clip_rect = match bf.base.style.get_effects().clip {
        ClipRectOrAuto::Rect(rect) => rect,
        _ => return None,
    };
    Some(physical_rect_to_rect(clip_rect.for_border_rect(bf.border_rect())))
}

fn push_clip_chain(
    scene: &mut MpScene,
    parent: makepad_browser_scene::MpClipChainId,
    spatial_id: makepad_browser_scene::MpSpatialId,
    kind: MpClipKind,
) -> makepad_browser_scene::MpClipChainId {
    let clip_id = scene.push_clip(MpClipNode { spatial_id, kind });
    scene.push_clip_chain(MpClipChain {
        parent: Some(parent),
        clips: vec![clip_id],
    })
}
