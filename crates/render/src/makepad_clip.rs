use makepad_widgets::*;

use crate::scene::{
    BackendClipExecutionKind, PaintContainerId, RenderScene, SceneClipGeometry, SceneClipId,
};

pub(crate) fn push_clip_chain(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    paint_container_id: PaintContainerId,
    clip_id: SceneClipId,
) -> usize {
    if clip_id == SceneClipId::INVALID {
        return 0;
    }
    let mut pushed = 0;
    let mut current = clip_id;
    let mut chain = Vec::new();
    while current != SceneClipId::INVALID {
        if let Some(execution) = scene.backend_clip_execution(current, paint_container_id) {
            chain.push(execution);
        }
        current = scene.clip_node(current).unwrap().parent_clip_id;
    }
    chain.reverse();
    for execution in chain {
        if push_backend_clip_execution(cx, execution) {
            pushed += 1;
        }
    }
    pushed
}

pub(crate) fn push_local_clip_chain(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    paint_container_id: PaintContainerId,
    clip_id: SceneClipId,
) -> usize {
    if clip_id == SceneClipId::INVALID {
        return 0;
    }
    let mut chain = Vec::new();
    let mut current = clip_id;
    let paint_spatial_node_id = scene.paint_container_spatial_node_id(paint_container_id);
    while current != SceneClipId::INVALID {
        let node = scene.clip_node(current).unwrap();
        if node.spatial_node_id != paint_spatial_node_id {
            break;
        }
        chain.push(node.geometry);
        current = node.parent_clip_id;
    }
    chain.reverse();
    for geometry in &chain {
        push_clip_geometry(cx, *geometry);
    }
    chain.len()
}

pub(crate) fn pop_clip_chain(cx: &mut Cx2d, pushed_count: usize) {
    for _ in 0..pushed_count {
        cx.pop_clip_rect();
    }
}

pub(crate) fn transform_point(matrix: &Mat4f, point: DVec2) -> DVec2 {
    let mapped = matrix.transform_vec4(vec4f(point.x as f32, point.y as f32, 0.0, 1.0));
    if mapped.w.abs() > 1e-6 {
        dvec2((mapped.x / mapped.w) as f64, (mapped.y / mapped.w) as f64)
    } else {
        dvec2(mapped.x as f64, mapped.y as f64)
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
        let mapped = transform_point(matrix, point);
        min_x = min_x.min(mapped.x);
        min_y = min_y.min(mapped.y);
        max_x = max_x.max(mapped.x);
        max_y = max_y.max(mapped.y);
    }
    Rect {
        pos: dvec2(min_x, min_y),
        size: dvec2((max_x - min_x).max(0.0), (max_y - min_y).max(0.0)),
    }
}

pub(crate) fn map_rect_between_paint_containers(
    scene: &RenderScene<'_>,
    from_paint_container_id: PaintContainerId,
    to_paint_container_id: PaintContainerId,
    rect: Rect,
) -> Rect {
    if from_paint_container_id == to_paint_container_id {
        return rect;
    }
    let world_rect = transform_rect(&scene.frame_world_transform(from_paint_container_id), rect);
    transform_rect(&scene.frame_world_inverse(to_paint_container_id), world_rect)
}

fn push_backend_clip_execution(
    cx: &mut Cx2d,
    execution: crate::scene::BackendClipExecution,
) -> bool {
    match execution.kind {
        BackendClipExecutionKind::DirectRect => {
            if let Some(rect) = execution.rect {
                cx.push_clip_rect(rect);
                true
            } else {
                false
            }
        }
        BackendClipExecutionKind::ProjectedQuadFallback | BackendClipExecutionKind::MaskFallback => {
            if let Some(rect) = execution.rect {
                cx.push_clip_rect(rect);
                true
            } else {
                false
            }
        }
    }
}

fn push_clip_geometry(cx: &mut Cx2d, geometry: SceneClipGeometry) {
    match geometry {
        SceneClipGeometry::Rect { rect } => cx.push_clip_rect(rect),
    }
}
