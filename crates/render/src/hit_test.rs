#![cfg_attr(not(test), allow(dead_code))]

//! Hit testing and scroll container lookup using the same semantic render scene.

use havi_fragment_semantics::{Fragment, OpaqueNode};
use makepad_compositor::MpBackfaceVisibility;
use makepad_widgets::*;

use crate::scene::{
    PaintContainerId, PaintContainerKind, RenderScene, SceneClipId, ScenePaintCommand,
    ScenePaintItem, SpatialNodeId, SpatialNodeSemantics,
};

pub(crate) fn hit_test(scene: &RenderScene<'_>, point_world: DVec2) -> Option<OpaqueNode> {
    hit_test_frame_reverse(scene, scene.root_paint_container_id(), point_world)
}

pub(crate) fn find_scroll_container(
    scene: &RenderScene<'_>,
    point_world: DVec2,
) -> Option<OpaqueNode> {
    find_scroll_container_in_frame_reverse(scene, scene.root_paint_container_id(), point_world)
}

fn hit_test_frame_reverse(
    scene: &RenderScene<'_>,
    paint_container_id: PaintContainerId,
    point_world: DVec2,
) -> Option<OpaqueNode> {
    if frame_backface_hidden(scene, paint_container_id) {
        return None;
    }
    let point_local = point_in_paint_container(scene, paint_container_id, point_world);

    for command in scene.frame_paint_list(paint_container_id).iter().rev() {
        match *command {
            ScenePaintCommand::ChildPaintContainer(child_paint_container_id) => {
                if let Some(hit) =
                    hit_test_frame_reverse(scene, child_paint_container_id, point_world)
                {
                    return Some(hit);
                }
            }
            ScenePaintCommand::Item(item_index) => {
                let item = &scene.frame_items(paint_container_id)[item_index];
                if !clip_chain_contains_point(scene, item.clip_id, point_world) {
                    continue;
                }
                if hit_test_item_local(item, point_local) {
                    if let Some(tag) = item.source.tag() {
                        return Some(tag.node);
                    }
                }
            }
        }
    }

    None
}

fn find_scroll_container_in_frame_reverse(
    scene: &RenderScene<'_>,
    paint_container_id: PaintContainerId,
    point_world: DVec2,
) -> Option<OpaqueNode> {
    if frame_backface_hidden(scene, paint_container_id) {
        return None;
    }
    for command in scene.frame_paint_list(paint_container_id).iter().rev() {
        if let ScenePaintCommand::ChildPaintContainer(child_paint_container_id) = *command {
            if let Some(hit) =
                find_scroll_container_in_frame_reverse(scene, child_paint_container_id, point_world)
            {
                return Some(hit);
            }
        }
    }

    let spatial_node = scene.spatial_node(scene.paint_container_spatial_node_id(paint_container_id));
    if let SpatialNodeSemantics::Scroll(scroll) = spatial_node.semantics {
        let clip_id = scene.effective_clip_chain_for_paint_container(paint_container_id);
        if !clip_chain_contains_point(scene, clip_id, point_world) {
            return None;
        }
        let point_local = point_in_paint_container(scene, paint_container_id, point_world);
        let x_in_scroll = point_local.x >= scroll.scroll_frame_rect.pos.x
            && point_local.x < scroll.scroll_frame_rect.pos.x + scroll.scroll_frame_rect.size.x;
        let y_in_scroll = point_local.y >= scroll.scroll_frame_rect.pos.y
            && point_local.y < scroll.scroll_frame_rect.pos.y + scroll.scroll_frame_rect.size.y;
        if (!scroll.sensitivity_x && !scroll.sensitivity_y)
            || (!scroll.sensitivity_x && !y_in_scroll)
            || (!scroll.sensitivity_y && !x_in_scroll)
        {
            return None;
        }
        if let Some(node_id) = scroll
            .external_scroll_node_id
            .or(scene.frame_owner_node_id(paint_container_id))
        {
            return Some(OpaqueNode(node_id));
        }
    }

    None
}

fn clip_chain_contains_point(
    scene: &RenderScene<'_>,
    clip_id: SceneClipId,
    point_world: DVec2,
) -> bool {
    if clip_id == SceneClipId::INVALID {
        return true;
    }

    let mut current = clip_id;
    while current != SceneClipId::INVALID {
        if !scene.clip_contains_world_point(current, point_world) {
            return false;
        }
        current = scene.clip_node(current).unwrap().parent_clip_id;
    }
    true
}

fn point_in_paint_container(
    scene: &RenderScene<'_>,
    paint_container_id: PaintContainerId,
    point_world: DVec2,
) -> DVec2 {
    point_in_spatial_node(
        scene,
        scene.paint_container_spatial_node_id(paint_container_id),
        point_world,
    )
}

fn point_in_spatial_node(
    scene: &RenderScene<'_>,
    spatial_node_id: SpatialNodeId,
    point_world: DVec2,
) -> DVec2 {
    transform_point(&scene.world_to_spatial_transform(spatial_node_id), point_world)
}

fn hit_test_item_local(item: &ScenePaintItem<'_>, point_local: DVec2) -> bool {
    let rect = match item.source {
        Fragment::Box(bf) | Fragment::Float(bf) => bf.border_rect(),
        Fragment::Text(tf) => tf.base.rect,
        Fragment::Image(img) => img.base.rect,
        Fragment::IFrame(iframe) => iframe.base.rect,
        Fragment::Positioning(_) | Fragment::AbsoluteOrFixedPositioned { .. } => return false,
    };

    point_in_rect(
        point_local,
        Rect {
            pos: dvec2(
                item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                item.local_origin.y + rect.origin.y.to_f32_px() as f64,
            ),
            size: dvec2(
                rect.size.width.to_f32_px() as f64,
                rect.size.height.to_f32_px() as f64,
            ),
        },
    )
}

fn frame_backface_hidden(scene: &RenderScene<'_>, paint_container_id: PaintContainerId) -> bool {
    let mut current = Some(scene.paint_container_spatial_node_id(paint_container_id));
    while let Some(spatial_node_id) = current {
        let spatial_node = scene.spatial_node(spatial_node_id);
        if let SpatialNodeSemantics::ReferenceFrame(data) = spatial_node.semantics {
            if matches!(data.backface_visibility, MpBackfaceVisibility::Hidden)
                && projected_signed_area(
                    &scene.spatial_to_world_transform(spatial_node_id),
                    frame_local_rect(scene, paint_container_id),
                )
                .map(|area| area < 0.0)
                .unwrap_or(false)
            {
                return true;
            }
        }
        current = spatial_node.parent;
    }
    false
}

fn frame_local_rect(scene: &RenderScene<'_>, paint_container_id: PaintContainerId) -> Rect {
    match scene.paint_container_kind(paint_container_id) {
        PaintContainerKind::IFrameRoot { size } => Rect {
            pos: dvec2(0.0, 0.0),
            size,
        },
        _ => scene
            .frame_items(paint_container_id)
            .iter()
            .filter_map(item_local_rect)
            .fold(None, union_rect)
            .unwrap_or(Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(1.0, 1.0),
            }),
    }
}

fn item_local_rect(item: &ScenePaintItem<'_>) -> Option<Rect> {
    let rect = match item.source {
        Fragment::Box(bf) | Fragment::Float(bf) => bf.border_rect(),
        Fragment::Text(tf) => tf.base.rect,
        Fragment::Image(img) => img.base.rect,
        Fragment::IFrame(iframe) => iframe.base.rect,
        Fragment::Positioning(_) | Fragment::AbsoluteOrFixedPositioned { .. } => return None,
    };
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

fn transform_point(matrix: &Mat4f, point: DVec2) -> DVec2 {
    let mapped = matrix.transform_vec4(vec4f(point.x as f32, point.y as f32, 0.0, 1.0));
    if mapped.w.abs() > 1e-6 {
        dvec2((mapped.x / mapped.w) as f64, (mapped.y / mapped.w) as f64)
    } else {
        dvec2(mapped.x as f64, mapped.y as f64)
    }
}

fn projected_signed_area(transform: &Mat4f, rect: Rect) -> Option<f32> {
    let p0 = project_point(transform, rect.pos)?;
    let p1 = project_point(transform, dvec2(rect.pos.x + rect.size.x, rect.pos.y))?;
    let p2 = project_point(transform, rect.pos + rect.size)?;
    Some((p1.x - p0.x) * (p2.y - p0.y) - (p1.y - p0.y) * (p2.x - p0.x))
}

fn project_point(transform: &Mat4f, point: DVec2) -> Option<Vec2f> {
    let clip = transform.transform_vec4(vec4f(point.x as f32, point.y as f32, 0.0, 1.0));
    if clip.w.abs() <= 1e-6 {
        return None;
    }
    Some(vec2(clip.x / clip.w, clip.y / clip.w))
}

fn point_in_rect(point: DVec2, rect: Rect) -> bool {
    point.x >= rect.pos.x
        && point.x < rect.pos.x + rect.size.x
        && point.y >= rect.pos.y
        && point.y < rect.pos.y + rect.size.y
}
