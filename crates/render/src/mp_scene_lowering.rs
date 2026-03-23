//! Legacy compositor lowering used only by the fallback render path.
//!
//! This path remains only for `HAVI_DISABLE_BROWSER_SCENE=1` and direct-builder
//! fallback failures.

use std::collections::HashMap;

use makepad_compositor::{
    MpBackfaceVisibility, MpBlendMode, MpClipNode, MpClipShape, MpEffectNode, MpEmbedNode,
    MpFilterSet, MpMaskSource, MpNode, MpNodeId, MpReferenceFrame, MpRenderer, MpScene,
    MpSceneRoot, MpSurface, MpSurfaceColorFormat, MpSurfaceNode, MpSurfaceSource,
};
use makepad_widgets::makepad_draw::draw_list_2d::DrawList2d;
use makepad_widgets::*;

use crate::makepad_builder::MakepadDrawState;
use crate::makepad_fragments::paint_fragment_item;
use crate::scene::{
    RenderBlendMode, RenderClipGeometry, RenderClipId, RenderMask, RenderNode, RenderNodeId,
    RenderPaintRun, RenderReferenceFrame, RenderReferenceFrameKind, RenderScene, RenderStickyInfo,
};
use crate::{SceneSurfaceCacheEntry, SceneSurfaceKey};

pub(crate) fn draw_render_scene(
    cx: &mut Cx2d,
    render_scene: &RenderScene<'_>,
    webview_origin: DVec2,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
) {
    if state.frame_draw_lists.renderer.is_none() {
        state.frame_draw_lists.renderer = Some(MpRenderer::new(cx.cx));
    }

    let host_rect = Rect {
        pos: webview_origin,
        size: root_viewport_size,
    };
    let scene = lower_render_scene_to_mp_scene(cx, render_scene, host_rect, Mat4f::identity(), state);
    if let Err(err) = state
        .frame_draw_lists
        .renderer
        .as_mut()
        .unwrap()
        .draw_scene(cx, &scene)
    {
        eprintln!("[havi][render] draw_scene error: {err:?}");
    }
}

fn lower_render_scene_to_mp_scene<'a>(
    cx: &mut Cx2d,
    render_scene: &'a RenderScene<'a>,
    host_rect: Rect,
    page_to_host: Mat4f,
    state: &mut MakepadDrawState<'_>,
) -> MpScene {
    let root_frame = render_scene.root_reference_frame();
    let mut scene = MpScene::new(MpSceneRoot {
        host_rect,
        page_to_host,
        clip: None,
    });

    let root_mp_id = scene.push(MpNode::ReferenceFrame(MpReferenceFrame {
        parent: None,
        clip: None,
        local_rect: root_frame.local_rect,
        transform: reference_frame_transform(root_frame),
        perspective: root_frame.perspective,
        transform_style: root_frame.transform_style,
        backface_visibility: root_frame.backface_visibility,
        flattens_descendants: root_frame.flattens_descendants,
    }));

    let mut node_map = HashMap::new();
    node_map.insert(render_scene.root_reference_frame_id(), root_mp_id);
    let mut clip_map = HashMap::new();

    for (index, node) in render_scene.nodes.iter().enumerate().skip(1) {
        let render_id = RenderNodeId(index);
        match node {
            RenderNode::ReferenceFrame(frame) => {
                let parent = frame
                    .parent
                    .and_then(|id| node_map.get(&id).copied())
                    .expect("reference frame parent must be lowered before child");
                let mp_id = scene.push(MpNode::ReferenceFrame(MpReferenceFrame {
                    parent: Some(parent),
                    clip: frame.clip.and_then(|id| clip_map.get(&id).copied()),
                    local_rect: frame.local_rect,
                    transform: reference_frame_transform(frame),
                    perspective: frame.perspective,
                    transform_style: frame.transform_style,
                    backface_visibility: frame.backface_visibility,
                    flattens_descendants: frame.flattens_descendants,
                }));
                node_map.insert(render_id, mp_id);
            }
            RenderNode::Clip(clip) => {
                let mp_id = scene.push(MpNode::Clip(MpClipNode {
                    parent: clip.parent.and_then(|id| node_map.get(&id).copied()),
                    prev: clip.prev.and_then(|id| clip_map.get(&id).copied()),
                    shape: clip_shape(&clip.geometry),
                }));
                clip_map.insert(RenderClipId(index), mp_id);
            }
            RenderNode::Effect(effect) => {
                let parent = node_map[&effect.parent];
                let mp_id = scene.push(MpNode::Effect(MpEffectNode {
                    parent,
                    clip: effect.clip.and_then(|id| clip_map.get(&id).copied()),
                    opacity: effect.opacity,
                    filter: MpFilterSet {
                        entries: effect.filter.entries.clone(),
                    },
                    blend_mode: mp_blend_mode(&effect.blend_mode),
                    is_isolated: effect.is_isolated,
                    mask: None,
                }));
                if let Some(mask) = effect.mask.as_ref() {
                    let mask_source = lower_mask_clip(&mut scene, mp_id, mask);
                    if let Some(MpNode::Effect(effect_node)) = scene.nodes.get_mut(mp_id) {
                        effect_node.mask = Some(mask_source);
                    }
                }
                node_map.insert(render_id, mp_id);
            }
            RenderNode::PaintRun(run) => {
                let parent = node_map[&run.parent];
                let texture = paint_render_run_surface(cx, render_scene, render_id, run, state);
                let mp_id = scene.push(MpNode::Surface(MpSurfaceNode {
                    parent,
                    clip: run.clip.and_then(|id| clip_map.get(&id).copied()),
                    local_rect: run.local_bounds,
                    source: MpSurfaceSource::SurfaceTexture(texture),
                    backface_visibility: MpBackfaceVisibility::Visible,
                }));
                node_map.insert(render_id, mp_id);
            }
            RenderNode::Embed(embed) => {
                let parent = node_map[&embed.parent];
                let child_scene = lower_render_scene_to_mp_scene(
                    cx,
                    &embed.child_scene,
                    Rect {
                        pos: dvec2(0.0, 0.0),
                        size: embed.local_rect.size,
                    },
                    Mat4f::identity(),
                    state,
                );
                let mp_id = scene.push(MpNode::Embed(MpEmbedNode {
                    parent,
                    clip: embed.clip.and_then(|id| clip_map.get(&id).copied()),
                    local_rect: embed.local_rect,
                    child_scene: Box::new(child_scene),
                }));
                node_map.insert(render_id, mp_id);
            }
        }
    }

    scene.root.clip = render_scene
        .root
        .clip
        .and_then(|id| clip_map.get(&id).copied());
    if let Some(MpNode::ReferenceFrame(root)) = scene.nodes.get_mut(root_mp_id) {
        root.clip = root_frame.clip.and_then(|id| clip_map.get(&id).copied());
    }

    scene
}

fn paint_render_run_surface(
    cx: &mut Cx2d,
    render_scene: &RenderScene<'_>,
    run_id: RenderNodeId,
    run: &RenderPaintRun<'_>,
    state: &mut MakepadDrawState<'_>,
) -> Texture {
    let key = SceneSurfaceKey {
        paint_container_id: run_id.0,
        run_index: render_scene as *const _ as usize,
    };
    let mut entry = state
        .frame_draw_lists
        .surfaces
        .remove(&key)
        .unwrap_or_else(|| SceneSurfaceCacheEntry {
            surface: MpSurface::new(
                cx.cx,
                run.local_bounds.size,
                MpSurfaceColorFormat::BgraU8,
                false,
            ),
            draw_list: DrawList2d::new(cx.cx),
        });
    entry.surface.resize(cx.cx, run.local_bounds.size);
    entry.surface.begin(cx, None);
    cx.set_pass_shift_scale(entry.surface.pass(), dvec2(0.0, 0.0), dvec2(1.0, 1.0));
    entry.draw_list.begin_always(cx);
    cx.begin_root_turtle_for_pass(Layout::default());
    for item in &run.items {
        paint_fragment_item(
            cx,
            item,
            item.local_origin - run.local_bounds.pos,
            state,
            1.0,
        );
    }
    cx.end_pass_sized_turtle();
    entry.draw_list.end(cx);
    entry.surface.end(cx);
    let texture = entry.surface.color_texture().clone();
    state.frame_draw_lists.surfaces.insert(key, entry);
    texture
}

fn clip_shape(geometry: &RenderClipGeometry) -> MpClipShape {
    match geometry {
        RenderClipGeometry::Rect { rect } => MpClipShape::Rect { rect: *rect },
        RenderClipGeometry::RoundedRect { rect, radius } => MpClipShape::RoundedRect {
            rect: *rect,
            radius: *radius,
        },
        RenderClipGeometry::PlaneSet { planes } => MpClipShape::PlaneSet {
            planes: planes.clone(),
        },
    }
}

fn lower_mask_clip(scene: &mut MpScene, parent: MpNodeId, mask: &RenderMask) -> MpMaskSource {
    let shape = match mask {
        RenderMask::Rect { rect } => MpClipShape::Rect { rect: *rect },
        RenderMask::RoundedRect { rect, radius } => MpClipShape::RoundedRect {
            rect: *rect,
            radius: *radius,
        },
        RenderMask::PlaneSet { planes } => MpClipShape::PlaneSet {
            planes: planes.clone(),
        },
    };
    let clip_id = scene.push(MpNode::Clip(MpClipNode {
        parent: Some(parent),
        prev: None,
        shape,
    }));
    MpMaskSource::Clip(clip_id)
}

fn mp_blend_mode(blend_mode: &RenderBlendMode) -> MpBlendMode {
    match blend_mode {
        RenderBlendMode::Normal => MpBlendMode::Normal,
        RenderBlendMode::Named(name) => MpBlendMode::Named(name.clone()),
    }
}

fn reference_frame_transform(frame: &RenderReferenceFrame) -> Mat4f {
    let mut transform = translation_matrix(frame.placement_origin.x as f32, frame.placement_origin.y as f32);
    if let Some(matrix) = frame.transform {
        transform = Mat4f::mul(&transform, &matrix);
    }
    match &frame.kind {
        RenderReferenceFrameKind::Root
        | RenderReferenceFrameKind::Transform
        | RenderReferenceFrameKind::IFrameRoot { .. } => {}
        RenderReferenceFrameKind::Scroll(info) => {
            transform = Mat4f::mul(
                &transform,
                &translation_matrix(-(info.scroll_offset.x as f32), -(info.scroll_offset.y as f32)),
            );
        }
        RenderReferenceFrameKind::Sticky(info) => {
            let offset = sticky_used_offset(info);
            transform = Mat4f::mul(
                &transform,
                &translation_matrix(offset.x as f32, offset.y as f32),
            );
        }
    }
    transform
}

fn sticky_used_offset(info: &RenderStickyInfo) -> DVec2 {
    if info.margins.top.is_none()
        && info.margins.right.is_none()
        && info.margins.bottom.is_none()
        && info.margins.left.is_none()
    {
        return dvec2(0.0, 0.0);
    }

    let mut sticky_rect = info.frame_rect;
    let mut sticky_offset = dvec2(0.0, 0.0);

    if let Some(margin) = info.margins.top {
        let top_viewport_edge = info.scroll_port_rect.pos.y + margin as f64;
        if sticky_rect.pos.y < top_viewport_edge {
            sticky_offset.y = top_viewport_edge - sticky_rect.pos.y;
        }
    }

    if sticky_offset.y <= 0.0 {
        if let Some(margin) = info.margins.bottom {
            sticky_rect.pos.y += sticky_offset.y;
            let bottom_viewport_edge =
                info.scroll_port_rect.pos.y + info.scroll_port_rect.size.y - margin as f64;
            let sticky_bottom = sticky_rect.pos.y + sticky_rect.size.y;
            if sticky_bottom > bottom_viewport_edge {
                sticky_offset.y += bottom_viewport_edge - sticky_bottom;
            }
        }
    }

    if let Some(margin) = info.margins.left {
        let left_viewport_edge = info.scroll_port_rect.pos.x + margin as f64;
        if sticky_rect.pos.x < left_viewport_edge {
            sticky_offset.x = left_viewport_edge - sticky_rect.pos.x;
        }
    }

    if sticky_offset.x <= 0.0 {
        if let Some(margin) = info.margins.right {
            sticky_rect.pos.x += sticky_offset.x;
            let right_viewport_edge =
                info.scroll_port_rect.pos.x + info.scroll_port_rect.size.x - margin as f64;
            let sticky_right = sticky_rect.pos.x + sticky_rect.size.x;
            if sticky_right > right_viewport_edge {
                sticky_offset.x += right_viewport_edge - sticky_right;
            }
        }
    }

    sticky_offset.y = sticky_offset
        .y
        .max(info.vertical_offset_bounds.min as f64)
        .min(info.vertical_offset_bounds.max as f64);
    sticky_offset.x = sticky_offset
        .x
        .max(info.horizontal_offset_bounds.min as f64)
        .min(info.horizontal_offset_bounds.max as f64);

    let frame_left = info.frame_rect.pos.x;
    let frame_top = info.frame_rect.pos.y;
    let frame_right = info.frame_rect.pos.x + info.frame_rect.size.x;
    let frame_bottom = info.frame_rect.pos.y + info.frame_rect.size.y;
    let cb_left = info.containing_block_rect.pos.x;
    let cb_top = info.containing_block_rect.pos.y;
    let cb_right = info.containing_block_rect.pos.x + info.containing_block_rect.size.x;
    let cb_bottom = info.containing_block_rect.pos.y + info.containing_block_rect.size.y;
    sticky_offset.x = sticky_offset.x.max(cb_left - frame_left).min(cb_right - frame_right);
    sticky_offset.y = sticky_offset.y.max(cb_top - frame_top).min(cb_bottom - frame_bottom);

    dvec2(sticky_offset.x, sticky_offset.y)
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
