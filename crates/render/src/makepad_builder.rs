//! Paint traversal and composition for a pre-built frame tree.

use makepad_widgets::makepad_draw::draw_list_2d::{DrawList2d, DrawListExt};
use makepad_widgets::*;

use crate::clip_tree::ClipTree;
use crate::frame_tree::{FrameId, FramePaintCommand, FrameTree};
use crate::makepad_clip::{pop_clip_chain, push_clip_chain, push_local_clip_chain};
use crate::makepad_effects::{
    begin_filter_pass, begin_opacity_pass, end_filter_pass, end_opacity_pass,
    frame_effects_for_node, frame_owner_bounds,
};
use crate::makepad_fragments::{paint_fragment_item, paint_selection_overlay};
use crate::{
    DrawBoxShadow, DrawFilterImage, DrawGradient, DrawRoundedColor, DrawVideoYuv, FilterState,
    FrameDrawList, FrameDrawListState, OpacityState, SelectionHighlight, TextureCache,
};

pub(crate) struct MakepadDrawState<'a> {
    pub draw_bg: &'a mut DrawColor,
    pub draw_text: &'a mut DrawText,
    pub draw_text_bold: &'a mut DrawText,
    pub draw_text_mono: &'a mut DrawText,
    pub draw_image: &'a mut DrawImage,
    pub texture_cache: &'a mut TextureCache,
    pub draw_rounded_bg: &'a mut DrawRoundedColor,
    pub draw_box_shadow: &'a mut DrawBoxShadow,
    pub draw_gradient: &'a mut DrawGradient,
    pub draw_video_yuv: &'a mut DrawVideoYuv,
    pub selection: Option<&'a SelectionHighlight>,
    pub opacity_state: &'a mut OpacityState,
    pub filter_state: &'a mut FilterState,
    pub draw_filter_image: &'a mut DrawFilterImage,
    pub frame_draw_lists: &'a mut FrameDrawListState,
    pub image_overrides: &'a havi_types::ImageOverrides,
}

pub(crate) fn paint_scene(
    cx: &mut Cx2d,
    frame_tree: &FrameTree<'_>,
    clip_tree: &ClipTree,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    paint_frame(cx, frame_tree, clip_tree, frame_tree.root, state, parent_opacity);
    paint_selection_overlay(cx, state);
}

fn paint_frame(
    cx: &mut Cx2d,
    frame_tree: &FrameTree<'_>,
    clip_tree: &ClipTree,
    frame_id: FrameId,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    if frame_id == frame_tree.root {
        paint_frame_with_effects(cx, frame_tree, clip_tree, frame_id, state, parent_opacity);
        return;
    }

    let frame = frame_tree.frame(frame_id);
    state
        .frame_draw_lists
        .entry(frame.key)
        .or_insert_with(|| FrameDrawList {
            draw_list: DrawList2d::new(cx.cx),
        })
        .draw_list
        .begin_always(cx);
    state
        .frame_draw_lists
        .get_mut(&frame.key)
        .unwrap()
        .draw_list
        .set_view_transform_self_only(cx.cx, &frame.matrix.world);
    paint_frame_with_effects(cx, frame_tree, clip_tree, frame_id, state, parent_opacity);
    state
        .frame_draw_lists
        .get_mut(&frame.key)
        .unwrap()
        .draw_list
        .end(cx);
}

fn paint_frame_with_effects(
    cx: &mut Cx2d,
    frame_tree: &FrameTree<'_>,
    clip_tree: &ClipTree,
    frame_id: FrameId,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    let (element_opacity, css_filters) = frame_effects_for_node(frame_tree, frame_id);
    let needs_filter = !css_filters.is_identity();
    let needs_opacity = element_opacity < 1.0 && !needs_filter;

    if frame_id != frame_tree.root {
        if let Some((node_id, bounds)) = frame_owner_bounds(frame_tree, frame_id) {
            let size = dvec2(bounds.size.x.max(1.0), bounds.size.y.max(1.0));
            if needs_filter {
                begin_filter_pass(cx, state, node_id, size, bounds.pos);
                paint_frame_contents(cx, frame_tree, clip_tree, frame_id, state, 1.0);
                end_filter_pass(
                    cx,
                    state,
                    node_id,
                    bounds,
                    parent_opacity * element_opacity * css_filters.filter_opacity,
                    &css_filters,
                );
                return;
            }
            if needs_opacity {
                begin_opacity_pass(cx, state, node_id, size, bounds.pos);
                paint_frame_contents(cx, frame_tree, clip_tree, frame_id, state, 1.0);
                end_opacity_pass(cx, state, node_id, bounds, parent_opacity * element_opacity);
                return;
            }
        }
    }

    paint_frame_contents(
        cx,
        frame_tree,
        clip_tree,
        frame_id,
        state,
        parent_opacity * element_opacity,
    );
}

fn paint_frame_contents(
    cx: &mut Cx2d,
    frame_tree: &FrameTree<'_>,
    clip_tree: &ClipTree,
    frame_id: FrameId,
    state: &mut MakepadDrawState<'_>,
    opacity: f32,
) {
    let paint_list = frame_tree.frame(frame_id).paint_list.clone();
    for command in paint_list {
        match command {
            FramePaintCommand::Item(item_index) => {
                let item = &frame_tree.frame(frame_id).items[item_index];
                let pushed = push_local_clip_chain(cx, clip_tree, frame_id, item.clip_id);
                paint_fragment_item(cx, item, state, opacity);
                pop_clip_chain(cx, pushed);
            }
            FramePaintCommand::ChildFrame(child_frame_id) => {
                let pushed = push_clip_chain(
                    cx,
                    frame_tree,
                    clip_tree,
                    frame_id,
                    frame_tree.frame(child_frame_id).clip_id,
                );
                paint_frame(cx, frame_tree, clip_tree, child_frame_id, state, opacity);
                pop_clip_chain(cx, pushed);
            }
        }
    }
}
