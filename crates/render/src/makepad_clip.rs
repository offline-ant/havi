use makepad_widgets::*;

use crate::clip_tree::{ClipId, ClipTree};
use crate::frame_tree::{FrameId, FrameTree};

pub(crate) fn push_clip_chain(
    cx: &mut Cx2d,
    frame_tree: &FrameTree<'_>,
    clip_tree: &ClipTree,
    frame_id: FrameId,
    clip_id: ClipId,
) -> usize {
    if clip_id == ClipId::INVALID {
        return 0;
    }
    let mut chain = Vec::new();
    let mut current = clip_id;
    while current != ClipId::INVALID {
        let node = clip_tree.get(current);
        chain.push(map_rect_between_frames(
            frame_tree,
            node.parent_frame_id,
            frame_id,
            node.rect,
        ));
        current = node.parent_clip_id;
    }
    chain.reverse();
    for rect in &chain {
        cx.push_clip_rect(*rect);
    }
    chain.len()
}

pub(crate) fn push_local_clip_chain(
    cx: &mut Cx2d,
    clip_tree: &ClipTree,
    frame_id: FrameId,
    clip_id: ClipId,
) -> usize {
    if clip_id == ClipId::INVALID {
        return 0;
    }
    let mut chain = Vec::new();
    let mut current = clip_id;
    while current != ClipId::INVALID {
        let node = clip_tree.get(current);
        if node.parent_frame_id != frame_id {
            break;
        }
        chain.push(node.rect);
        current = node.parent_clip_id;
    }
    chain.reverse();
    for rect in &chain {
        cx.push_clip_rect(*rect);
    }
    chain.len()
}

pub(crate) fn pop_clip_chain(cx: &mut Cx2d, pushed_count: usize) {
    for _ in 0..pushed_count {
        cx.pop_clip_rect();
    }
}

pub(crate) fn transform_rect(matrix: &Mat4f, rect: Rect) -> Rect {
    let points = [
        dvec2(rect.pos.x, rect.pos.y),
        dvec2(rect.pos.x + rect.size.x, rect.pos.y),
        dvec2(rect.pos.x, rect.pos.y + rect.size.y),
        dvec2(rect.pos.x + rect.size.x, rect.pos.y + rect.size.y),
    ];
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for point in points {
        let mapped = matrix.transform_vec4(vec4f(point.x as f32, point.y as f32, 0.0, 1.0));
        let x = if mapped.w.abs() > 1e-6 { mapped.x / mapped.w } else { mapped.x } as f64;
        let y = if mapped.w.abs() > 1e-6 { mapped.y / mapped.w } else { mapped.y } as f64;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    Rect {
        pos: dvec2(min_x, min_y),
        size: dvec2((max_x - min_x).max(0.0), (max_y - min_y).max(0.0)),
    }
}

pub(crate) fn map_rect_between_frames(
    frame_tree: &FrameTree<'_>,
    from_frame_id: FrameId,
    to_frame_id: FrameId,
    rect: Rect,
) -> Rect {
    if from_frame_id == to_frame_id {
        return rect;
    }
    let world_rect = transform_rect(&frame_tree.frame(from_frame_id).matrix.world, rect);
    transform_rect(&frame_tree.frame(to_frame_id).matrix.world_inverse, world_rect)
}
