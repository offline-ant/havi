#![cfg_attr(not(test), allow(dead_code))]

//! Hit testing and scroll container lookup using the same render scene as painting.

use havi_fragment_semantics::{Fragment, OpaqueNode};
use makepad_widgets::*;

use crate::makepad_clip::transform_point;
use crate::scene::{
    PaintContainerId, RenderScene, SceneClipId, ScenePaintCommand, ScenePaintItem,
    SpatialNodeSemantics,
};

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
    let point_local = point_in_paint_container(scene, paint_container_id, point_world);

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

    let spatial_node = scene.spatial_node(scene.paint_container_spatial_node_id(paint_container_id));
    if let SpatialNodeSemantics::Scroll(scroll) = spatial_node.semantics {
        let clip_id = scene.effective_clip_chain_for_paint_container(paint_container_id);
        if !clip_chain_contains_point(scene, clip_id, point_world) {
            return None;
        }
        if let Some(node_id) = scroll.external_scroll_node_id.or(scene.frame_owner_node_id(paint_container_id)) {
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
        let node = scene.clip_node(current).unwrap();
        current = node.parent_clip_id;
    }
    true
}

fn point_in_paint_container(
    scene: &RenderScene<'_>,
    paint_container_id: PaintContainerId,
    point_world: DVec2,
) -> DVec2 {
    point_in_spatial_node(scene, scene.paint_container_spatial_node_id(paint_container_id), point_world)
}

fn point_in_spatial_node(
    scene: &RenderScene<'_>,
    spatial_node_id: crate::scene::SpatialNodeId,
    point_world: DVec2,
) -> DVec2 {
    transform_point(&scene.world_to_spatial_transform(spatial_node_id), point_world)
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
