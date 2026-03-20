#![cfg_attr(not(test), allow(dead_code))]

//! Hit testing and scroll container lookup using the same render scene as painting.

use havi_fragment_semantics::{Fragment, OpaqueNode};
use makepad_widgets::*;

use crate::frame_tree::FramePaintCommand;
use crate::makepad_clip::transform_point;
use crate::scene::{RenderScene, SpatialNodeKind};

pub(crate) fn hit_test(
    scene: &RenderScene<'_>,
    point_world: DVec2,
) -> Option<OpaqueNode> {
    hit_test_frame_reverse(scene, scene.root_frame_id(), point_world)
}

pub(crate) fn find_scroll_container(
    scene: &RenderScene<'_>,
    point_world: DVec2,
) -> Option<OpaqueNode> {
    find_scroll_container_in_frame_reverse(scene, scene.root_frame_id(), point_world)
}

fn hit_test_frame_reverse(
    scene: &RenderScene<'_>,
    frame_id: crate::frame_tree::FrameId,
    point_world: DVec2,
) -> Option<OpaqueNode> {
    let frame = scene.frame(frame_id);
    let point_local = transform_point(&frame.matrix.world_inverse, point_world);

    for command in frame.paint_list.iter().rev() {
        match *command {
            FramePaintCommand::ChildFrame(child_id) => {
                if let Some(hit) = hit_test_frame_reverse(scene, child_id, point_world) {
                    return Some(hit);
                }
            }
            FramePaintCommand::Item(item_index) => {
                let item = &frame.items[item_index];
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
    frame_id: crate::frame_tree::FrameId,
    point_world: DVec2,
) -> Option<OpaqueNode> {
    let frame = scene.frame(frame_id);

    for command in frame.paint_list.iter().rev() {
        if let FramePaintCommand::ChildFrame(child_id) = *command {
            if let Some(hit) = find_scroll_container_in_frame_reverse(scene, child_id, point_world) {
                return Some(hit);
            }
        }
    }

    if scene.spatial_node(crate::scene::SpatialNodeId(frame_id)).kind == SpatialNodeKind::Scroll {
        if !clip_chain_contains_point(scene, frame.clip_id, point_world) {
            return None;
        }
        if let Some(node_id) = frame.owner_node_id {
            return Some(OpaqueNode(node_id));
        }
    }

    None
}

fn clip_chain_contains_point(
    scene: &RenderScene<'_>,
    clip_id: crate::clip_tree::ClipId,
    point_world: DVec2,
) -> bool {
    if clip_id == crate::clip_tree::ClipId::INVALID {
        return true;
    }

    let mut current = clip_id;
    while current != crate::clip_tree::ClipId::INVALID {
        let node = scene.clip_tree.get(current);
        let frame = scene.frame(node.parent_frame_id);
        let point_local = transform_point(&frame.matrix.world_inverse, point_world);
        if !point_in_rect(point_local, node.rect) {
            return false;
        }
        current = node.parent_clip_id;
    }
    true
}

fn hit_test_item_local(
    item: &crate::frame_tree::FramePaintItem<'_>,
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
