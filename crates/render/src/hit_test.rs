#![cfg_attr(not(test), allow(dead_code))]

//! Hit testing and scroll container lookup using the same render scene as painting.

use havi_fragment_semantics::{Fragment, OpaqueNode};
use makepad_widgets::*;

use crate::makepad_clip::transform_point;
use crate::scene::{PaintContainerId, RenderScene, SceneClipId, ScenePaintCommand, ScenePaintItem, SpatialNodeKind};

pub(crate) fn hit_test(
    scene: &RenderScene<'_>,
    point_world: DVec2,
) -> Option<OpaqueNode> {
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
    let point_local = transform_point(&scene.frame_world_inverse(paint_container_id), point_world);

    for command in scene.frame_paint_list(paint_container_id).iter().rev() {
        match *command {
            ScenePaintCommand::ChildPaintContainer(child_paint_container_id) => {
                if let Some(hit) = hit_test_frame_reverse(scene, child_paint_container_id, point_world) {
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
    for command in scene.frame_paint_list(paint_container_id).iter().rev() {
        if let ScenePaintCommand::ChildPaintContainer(child_paint_container_id) = *command {
            if let Some(hit) = find_scroll_container_in_frame_reverse(scene, child_paint_container_id, point_world) {
                return Some(hit);
            }
        }
    }

    if scene.spatial_node(scene.paint_container_spatial_node_id(paint_container_id)).kind == SpatialNodeKind::Scroll {
        if !clip_chain_contains_point(scene, scene.frame_clip_id(paint_container_id), point_world) {
            return None;
        }
        if let Some(node_id) = scene.frame_owner_node_id(paint_container_id) {
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
        let node = scene.clip_node(current).unwrap();
        let point_local = transform_point(
            &scene.spatial_node(node.parent_spatial_node_id).world_inverse,
            point_world,
        );
        if !point_in_rect(point_local, node.rect) {
            return false;
        }
        current = node.parent_clip_id;
    }
    true
}

fn hit_test_item_local(
    item: &ScenePaintItem<'_>,
    point_local: DVec2,
) -> bool {
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

fn point_in_rect(point: DVec2, rect: Rect) -> bool {
    point.x >= rect.pos.x
        && point.x < rect.pos.x + rect.size.x
        && point.y >= rect.pos.y
        && point.y < rect.pos.y + rect.size.y
}
