//! Paint traversal and composition for a pre-built render scene.

use std::collections::HashMap;

use makepad_compositor::{MpCompositedQuad, MpCompositor, MpSurface, MpSurfaceColorFormat};
use makepad_widgets::makepad_draw::draw_list_2d::{DrawList2d, DrawListExt};
use makepad_widgets::*;

use crate::compositor_scene::CompositorSurfaceId;
use crate::frame_tree::{FrameId, FramePaintCommand, FramePaintItem};
use crate::makepad_clip::{pop_clip_chain, push_clip_chain, push_local_clip_chain, transform_rect};
use crate::makepad_effects::{
    begin_filter_pass, begin_opacity_pass, end_filter_pass, end_opacity_pass, frame_effects_for_node,
};
use crate::makepad_fragments::{paint_fragment_item, paint_selection_overlay};
use crate::render_plan::RenderParticipation;
use crate::scene::RenderScene;
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

struct CompositorRuntime {
    compositor: MpCompositor,
    surfaces: HashMap<CompositorSurfaceId, MpSurface>,
}

impl CompositorRuntime {
    fn new(cx: &mut Cx) -> Self {
        Self {
            compositor: MpCompositor::new(cx),
            surfaces: HashMap::new(),
        }
    }

    fn ensure_surface(
        &mut self,
        cx: &mut Cx,
        surface_id: CompositorSurfaceId,
        size: DVec2,
        with_depth: bool,
    ) {
        self.surfaces
            .entry(surface_id)
            .and_modify(|surface| surface.resize(cx, size))
            .or_insert_with(|| MpSurface::new(cx, size, MpSurfaceColorFormat::BgraU8, with_depth));
    }

    fn begin_surface(
        &mut self,
        cx: &mut Cx2d,
        surface_id: CompositorSurfaceId,
        size: DVec2,
        with_depth: bool,
        shift: DVec2,
    ) {
        self.ensure_surface(cx.cx, surface_id, size, with_depth);
        let surface = self.surfaces.get_mut(&surface_id).unwrap();
        surface.begin(cx, None);
        cx.set_pass_shift_scale(surface.pass(), shift, dvec2(1.0, 1.0));
    }

    fn end_surface(&mut self, cx: &mut Cx2d, surface_id: CompositorSurfaceId) {
        self.surfaces.get_mut(&surface_id).unwrap().end(cx);
    }

    fn surface_texture(&self, surface_id: CompositorSurfaceId) -> Texture {
        self.surfaces
            .get(&surface_id)
            .unwrap()
            .color_texture()
            .clone()
    }
}

pub(crate) fn paint_scene(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    let mut runtime = CompositorRuntime::new(cx.cx);
    paint_frame_target(
        cx,
        scene,
        &mut runtime,
        scene.root_frame_id(),
        None,
        None,
        root_viewport_size,
        state,
        parent_opacity,
    );
    paint_selection_overlay(cx, state);
}

fn paint_frame_target(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    runtime: &mut CompositorRuntime,
    frame_id: FrameId,
    active_surface_id: Option<CompositorSurfaceId>,
    space_root_frame_id: Option<FrameId>,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    thread_local! { static DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
    DEPTH.with(|d| {
        let depth = d.get() + 1;
        d.set(depth);
        if depth % 200 == 0 {
            eprintln!("[makepad-builder] paint_frame_target depth={} frame_id={} total_frames={}", depth, frame_id, scene.frame_count());
        }
        if depth > 2000 {
            eprintln!("[makepad-builder] ABORTING paint_frame_target depth={} — likely infinite recursion", depth);
            std::process::abort();
        }
    });
    let frame_surface_id = scene.frame_surface(frame_id);
    let redirects_to_surface = frame_surface_id.is_some() && frame_surface_id != active_surface_id;
    let participation = scene.frame_participation(frame_id);

    match participation {
        RenderParticipation::Compositor { .. } if redirects_to_surface => {
            paint_compositor_surface(
                cx,
                scene,
                runtime,
                frame_surface_id.unwrap(),
                frame_id,
                space_root_frame_id,
                root_viewport_size,
                state,
                parent_opacity,
            );
        }
        RenderParticipation::Direct2d | RenderParticipation::Compositor { .. } => {
            paint_frame_direct_2d(
                cx,
                scene,
                runtime,
                frame_id,
                active_surface_id,
                space_root_frame_id,
                root_viewport_size,
                state,
                parent_opacity,
            );
        }
    }
    DEPTH.with(|d| d.set(d.get() - 1));
}

fn paint_compositor_surface(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    runtime: &mut CompositorRuntime,
    surface_id: CompositorSurfaceId,
    surface_root_frame_id: FrameId,
    parent_space_root_frame_id: Option<FrameId>,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    let Some(local_bounds) = frame_subtree_bounds_in_space(
        scene,
        surface_root_frame_id,
        surface_root_frame_id,
    ) else {
        return;
    };
    if local_bounds.size.x <= 0.0 || local_bounds.size.y <= 0.0 {
        return;
    }

    let with_depth = matches!(
        scene.frame_participation(surface_root_frame_id),
        RenderParticipation::Compositor { .. }
    );
    runtime.begin_surface(
        cx,
        surface_id,
        local_bounds.size,
        with_depth,
        local_bounds.pos,
    );
    paint_frame_target(
        cx,
        scene,
        runtime,
        surface_root_frame_id,
        Some(surface_id),
        Some(surface_root_frame_id),
        root_viewport_size,
        state,
        1.0,
    );
    runtime.end_surface(cx, surface_id);

    let mut quad = MpCompositedQuad::new(
        runtime.surface_texture(surface_id),
        Rect {
            pos: dvec2(0.0, 0.0),
            size: local_bounds.size,
        },
    );
    let frame_transform = frame_transform_in_space(scene, parent_space_root_frame_id, surface_root_frame_id);
    quad.transform = Mat4f::mul(
        &frame_transform,
        &translation_matrix(local_bounds.pos.x as f32, local_bounds.pos.y as f32),
    );
    quad.opacity = parent_opacity.clamp(0.0, 1.0);
    quad.depth_write = true;
    runtime.compositor.draw_quad(cx, &quad);
}

fn paint_frame_direct_2d(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    runtime: &mut CompositorRuntime,
    frame_id: FrameId,
    active_surface_id: Option<CompositorSurfaceId>,
    space_root_frame_id: Option<FrameId>,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    let frame = scene.frame(frame_id);
    let pass_size = cx.current_pass_size();

    if frame_id == scene.root_frame_id() {
        cx.begin_page_root_turtle(dvec2(0.0, 0.0), root_viewport_size, Layout::default());
        state.draw_bg.color = vec4(1.0, 1.0, 1.0, 1.0);
        state.draw_bg.draw_abs(
            cx,
            Rect {
                pos: dvec2(0.0, 0.0),
                size: root_viewport_size,
            },
        );
        paint_frame_with_effects(
            cx,
            scene,
            runtime,
            frame_id,
            active_surface_id,
            space_root_frame_id,
            root_viewport_size,
            state,
            parent_opacity,
        );
        cx.end_pass_sized_turtle();
        return;
    }

    let frame_draw_list = state
        .frame_draw_lists
        .entry(frame.key)
        .or_insert_with(|| FrameDrawList {
            draw_list: DrawList2d::new(cx.cx),
        });
    frame_draw_list.draw_list.begin_always(cx);
    cx.begin_unclipped_root_turtle(pass_size, Layout::default());
    state
        .frame_draw_lists
        .get_mut(&frame.key)
        .unwrap()
        .draw_list
        .set_view_transform_self_only(
            cx.cx,
            &frame_transform_in_space(scene, space_root_frame_id, frame_id),
        );
    paint_frame_with_effects(
        cx,
        scene,
        runtime,
        frame_id,
        active_surface_id,
        space_root_frame_id,
        root_viewport_size,
        state,
        parent_opacity,
    );
    cx.end_pass_sized_turtle_no_clip();
    state
        .frame_draw_lists
        .get_mut(&frame.key)
        .unwrap()
        .draw_list
        .end(cx);
}

fn paint_frame_with_effects(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    runtime: &mut CompositorRuntime,
    frame_id: FrameId,
    active_surface_id: Option<CompositorSurfaceId>,
    space_root_frame_id: Option<FrameId>,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    let (element_opacity, css_filters) = frame_effects_for_node(scene, frame_id);
    let needs_filter = !css_filters.is_identity();
    let needs_opacity = element_opacity < 1.0 && !needs_filter;

    if frame_id != scene.root_frame_id() {
        if let Some((node_id, bounds)) = frame_owner_bounds_in_space(scene, frame_id, space_root_frame_id) {
            let size = dvec2(bounds.size.x.max(1.0), bounds.size.y.max(1.0));
            if needs_filter {
                begin_filter_pass(cx, state, node_id, size, bounds.pos);
                paint_frame_contents(
                    cx,
                    scene,
                    runtime,
                    frame_id,
                    active_surface_id,
                    space_root_frame_id,
                    root_viewport_size,
                    state,
                    1.0,
                );
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
                paint_frame_contents(
                    cx,
                    scene,
                    runtime,
                    frame_id,
                    active_surface_id,
                    space_root_frame_id,
                    root_viewport_size,
                    state,
                    1.0,
                );
                end_opacity_pass(cx, state, node_id, bounds, parent_opacity * element_opacity);
                return;
            }
        }
    }

    paint_frame_contents(
        cx,
        scene,
        runtime,
        frame_id,
        active_surface_id,
        space_root_frame_id,
        root_viewport_size,
        state,
        parent_opacity * element_opacity,
    );
}

fn paint_frame_contents(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    runtime: &mut CompositorRuntime,
    frame_id: FrameId,
    active_surface_id: Option<CompositorSurfaceId>,
    space_root_frame_id: Option<FrameId>,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
    opacity: f32,
) {
    let paint_list = scene.frame_paint_list(frame_id).to_vec();
    for command in paint_list {
        match command {
            FramePaintCommand::Item(item_index) => {
                let item = &scene.frame_items(frame_id)[item_index];
                let pushed = push_local_clip_chain(cx, scene, frame_id, item.clip_id);
                paint_fragment_item(cx, item, state, opacity);
                pop_clip_chain(cx, pushed);
            }
            FramePaintCommand::ChildFrame(child_frame_id) => {
                let child_parent_surface_id = scene.frame_parent_surface(child_frame_id);
                if child_parent_surface_id.is_some() && child_parent_surface_id != active_surface_id {
                    continue;
                }
                let pushed = push_clip_chain(
                    cx,
                    scene,
                    frame_id,
                    scene.frame(child_frame_id).clip_id,
                );
                paint_frame_target(
                    cx,
                    scene,
                    runtime,
                    child_frame_id,
                    active_surface_id,
                    space_root_frame_id,
                    root_viewport_size,
                    state,
                    opacity,
                );
                pop_clip_chain(cx, pushed);
            }
        }
    }
}

fn frame_transform_in_space(
    scene: &RenderScene<'_>,
    space_root_frame_id: Option<FrameId>,
    frame_id: FrameId,
) -> Mat4f {
    match space_root_frame_id {
        Some(space_root_frame_id) => Mat4f::mul(
            &scene.frame_world_inverse(space_root_frame_id),
            &scene.frame_world_transform(frame_id),
        ),
        None => scene.frame_world_transform(frame_id),
    }
}

fn frame_owner_bounds_in_space(
    scene: &RenderScene<'_>,
    frame_id: FrameId,
    space_root_frame_id: Option<FrameId>,
) -> Option<(usize, Rect)> {
    let frame = scene.frame(frame_id);
    let owner_node_id = frame.owner_node_id?;
    let transform = frame_transform_in_space(scene, space_root_frame_id, frame_id);
    for item in scene.frame_items(frame_id) {
        if let Some(local_rect) = frame_paint_item_local_rect(item) {
            return Some((owner_node_id, transform_rect(&transform, local_rect)));
        }
    }
    None
}

fn frame_paint_item_local_rect(item: &FramePaintItem<'_>) -> Option<Rect> {
    match item.source {
        havi_fragment_semantics::Fragment::Box(bf) | havi_fragment_semantics::Fragment::Float(bf) => {
            let rect = bf.border_rect();
            Some(Rect {
                pos: dvec2(
                    item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                    item.local_origin.y + rect.origin.y.to_f32_px() as f64,
                ),
                size: dvec2(
                    rect.size.width.to_f32_px() as f64,
                    rect.size.height.to_f32_px() as f64,
                ),
            })
        }
        havi_fragment_semantics::Fragment::Text(tf) => {
            let rect = tf.base.rect;
            Some(Rect {
                pos: dvec2(
                    item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                    item.local_origin.y + rect.origin.y.to_f32_px() as f64,
                ),
                size: dvec2(
                    rect.size.width.to_f32_px() as f64,
                    rect.size.height.to_f32_px() as f64,
                ),
            })
        }
        havi_fragment_semantics::Fragment::Image(img) => {
            let rect = img.base.rect;
            Some(Rect {
                pos: dvec2(
                    item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                    item.local_origin.y + rect.origin.y.to_f32_px() as f64,
                ),
                size: dvec2(
                    rect.size.width.to_f32_px() as f64,
                    rect.size.height.to_f32_px() as f64,
                ),
            })
        }
        havi_fragment_semantics::Fragment::IFrame(iframe) => {
            let rect = iframe.base.rect;
            Some(Rect {
                pos: dvec2(
                    item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                    item.local_origin.y + rect.origin.y.to_f32_px() as f64,
                ),
                size: dvec2(
                    rect.size.width.to_f32_px() as f64,
                    rect.size.height.to_f32_px() as f64,
                ),
            })
        }
        havi_fragment_semantics::Fragment::Positioning(_) | havi_fragment_semantics::Fragment::AbsoluteOrFixedPositioned { .. } => None,
    }
}

fn frame_subtree_bounds_in_space(
    scene: &RenderScene<'_>,
    space_root_frame_id: FrameId,
    frame_id: FrameId,
) -> Option<Rect> {
    let mut bounds = None;
    let paint_list = scene.frame_paint_list(frame_id).to_vec();
    for command in paint_list {
        match command {
            FramePaintCommand::Item(item_index) => {
                if let Some(local_rect) = frame_paint_item_local_rect(&scene.frame_items(frame_id)[item_index]) {
                    let mapped = transform_rect(
                        &frame_transform_in_space(scene, Some(space_root_frame_id), frame_id),
                        local_rect,
                    );
                    bounds = union_rect(bounds, mapped);
                }
            }
            FramePaintCommand::ChildFrame(child_frame_id) => {
                let child_is_separate_surface = scene
                    .frame_surface(child_frame_id)
                    .is_some_and(|surface_id| Some(surface_id) != scene.frame_surface(frame_id));
                let child_bounds = if child_is_separate_surface {
                    frame_subtree_bounds_in_space(
                        scene,
                        child_frame_id,
                        child_frame_id,
                    )
                    .map(|rect| {
                        transform_rect(
                            &frame_transform_in_space(scene, Some(space_root_frame_id), child_frame_id),
                            rect,
                        )
                    })
                } else {
                    frame_subtree_bounds_in_space(
                        scene,
                        space_root_frame_id,
                        child_frame_id,
                    )
                };
                if let Some(child_bounds) = child_bounds {
                    bounds = union_rect(bounds, child_bounds);
                }
            }
        }
    }
    bounds
}

fn translation_matrix(tx: f32, ty: f32) -> Mat4f {
    Mat4f {
        v: [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            tx, ty, 0.0, 1.0,
        ],
    }
}

fn union_rect(current: Option<Rect>, next: Rect) -> Option<Rect> {
    match current {
        None => Some(next),
        Some(current) => {
            let min_x = current.pos.x.min(next.pos.x);
            let min_y = current.pos.y.min(next.pos.y);
            let max_x = (current.pos.x + current.size.x).max(next.pos.x + next.size.x);
            let max_y = (current.pos.y + current.size.y).max(next.pos.y + next.size.y);
            Some(Rect {
                pos: dvec2(min_x, min_y),
                size: dvec2((max_x - min_x).max(0.0), (max_y - min_y).max(0.0)),
            })
        }
    }
}
