use std::collections::HashMap;

use makepad_compositor::{MpCompositedQuad, MpCompositor, MpSurface, MpSurfaceColorFormat};
use makepad_widgets::*;

use crate::scene::{
    BackendClipExecution, BackendClipExecutionKind, BackendClipPlanes, BackendProjectedClipLimit,
    PaintContainerId, RenderScene, SceneClipGeometry, SceneClipId,
};

#[derive(Clone, Debug)]
pub(crate) struct MaskClipNode {
    pub local_rect: Rect,
    pub local_quad: [DVec2; 4],
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MaskClipChain {
    pub nodes: Vec<MaskClipNode>,
}

pub(crate) struct MaskRuntime {
    compositor: MpCompositor,
    mask_surfaces: HashMap<PaintContainerId, MpSurface>,
}

impl MaskRuntime {
    pub(crate) fn new(cx: &mut Cx) -> Self {
        Self {
            compositor: MpCompositor::new(cx),
            mask_surfaces: HashMap::new(),
        }
    }

    fn ensure_mask_surface(&mut self, cx: &mut Cx, paint_container_id: PaintContainerId, size: DVec2) {
        self.mask_surfaces
            .entry(paint_container_id)
            .and_modify(|surface| surface.resize(cx, size))
            .or_insert_with(|| MpSurface::new(cx, size, MpSurfaceColorFormat::BgraU8, false));
    }

    fn begin_mask_surface(
        &mut self,
        cx: &mut Cx2d,
        paint_container_id: PaintContainerId,
        size: DVec2,
    ) {
        self.ensure_mask_surface(cx.cx, paint_container_id, size);
        let surface = self.mask_surfaces.get_mut(&paint_container_id).unwrap();
        surface.begin(cx, None);
        cx.set_pass_shift_scale(surface.pass(), dvec2(0.0, 0.0), dvec2(1.0, 1.0));
    }

    fn end_mask_surface(&mut self, cx: &mut Cx2d, paint_container_id: PaintContainerId) {
        self.mask_surfaces.get_mut(&paint_container_id).unwrap().end(cx);
    }

    pub(crate) fn begin_mask_chain(
        &mut self,
        cx: &mut Cx2d,
        paint_container_id: PaintContainerId,
        chain: &MaskClipChain,
    ) -> Option<Texture> {
        if chain.nodes.is_empty() {
            return None;
        }
        let size = cx.current_pass_size();
        if size.x <= 0.0 || size.y <= 0.0 {
            return None;
        }
        self.begin_mask_surface(cx, paint_container_id, size);
        cx.begin_unclipped_root_turtle_for_pass(Layout::default());
        for node in &chain.nodes {
            let mut quad = MpCompositedQuad::new(white_mask_texture(cx), node.local_rect);
            quad.transform = quad_transform_from_local_quad(node.local_rect, node.local_quad);
            quad.premultiplied = false;
            quad.depth_write = false;
            self.compositor.draw_quad(cx, &quad);
        }
        cx.end_pass_sized_turtle_no_clip();
        self.end_mask_surface(cx, paint_container_id);
        Some(self.mask_surfaces.get(&paint_container_id).unwrap().color_texture().clone())
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ClassifiedClipChain {
    pub executions: Vec<BackendClipExecution>,
    pub push_result: ClipPushResult,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ClipPushResult {
    pub rect_pushes: usize,
    pub projected_quad_clip_planes: Option<BackendClipPlanes>,
    pub projected_clip_limit: Option<BackendProjectedClipLimit>,
    pub used_projected_quad_fallback: bool,
    pub used_mask_fallback: bool,
    pub mask_chain: Option<MaskClipChain>,
}

#[derive(Clone, Copy, Debug)]
enum BackendClipPush {
    None,
    Rect,
    ProjectedQuad,
    ProjectedQuadFallback,
    MaskFallback,
}

pub(crate) fn push_clip_chain(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    paint_container_id: PaintContainerId,
    clip_id: SceneClipId,
) -> ClipPushResult {
    let result = classify_clip_chain(scene, paint_container_id, clip_id);
    for execution in &result.executions {
        match push_backend_clip_execution(cx, *execution) {
            BackendClipPush::Rect
            | BackendClipPush::ProjectedQuad
            | BackendClipPush::ProjectedQuadFallback
            | BackendClipPush::MaskFallback
            | BackendClipPush::None => {}
        }
    }
    result.push_result
}

pub(crate) fn classify_clip_chain(
    scene: &RenderScene<'_>,
    paint_container_id: PaintContainerId,
    clip_id: SceneClipId,
) -> ClassifiedClipChain {
    if clip_id == SceneClipId::INVALID {
        return ClassifiedClipChain::default();
    }
    let mut push_result = ClipPushResult::default();
    let mut current = clip_id;
    let mut executions = Vec::new();
    let mut mask_nodes = Vec::new();
    while current != SceneClipId::INVALID {
        if let Some(execution) = scene.backend_clip_execution(current, paint_container_id) {
            match execution.kind {
                BackendClipExecutionKind::DirectRect => push_result.rect_pushes += execution.rect.is_some() as usize,
                BackendClipExecutionKind::ProjectedQuadFallback => {
                    if let Some(clip_planes) = execution.clip_planes {
                        push_result.projected_quad_clip_planes = Some(clip_planes);
                    }
                    if let Some(limit) = execution.projected_clip_limit {
                        push_result.projected_clip_limit = Some(limit);
                    }
                    push_result.used_projected_quad_fallback = true;
                }
                BackendClipExecutionKind::MaskFallback => {
                    if let Some(limit) = execution.projected_clip_limit {
                        push_result.projected_clip_limit = Some(limit);
                    }
                    if let (Some(rect), Some(quad)) = (execution.rect, execution.quad) {
                        mask_nodes.push(MaskClipNode {
                            local_rect: rect,
                            local_quad: quad,
                        });
                    }
                    push_result.used_mask_fallback = true;
                }
            }
            executions.push(execution);
        }
        current = scene.clip_node(current).unwrap().parent_clip_id;
    }
    executions.reverse();
    mask_nodes.reverse();
    if !mask_nodes.is_empty() {
        push_result.mask_chain = Some(MaskClipChain { nodes: mask_nodes });
    }
    ClassifiedClipChain {
        executions,
        push_result,
    }
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
        match node.kind {
            crate::scene::SceneClipKind::Overflow
            | crate::scene::SceneClipKind::OverflowClip
            | crate::scene::SceneClipKind::CssClip => {
                chain.push(node.geometry);
            }
        }
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

fn push_backend_clip_execution(cx: &mut Cx2d, execution: BackendClipExecution) -> BackendClipPush {
    match execution.kind {
        BackendClipExecutionKind::DirectRect => {
            if let Some(rect) = execution.rect {
                cx.push_clip_rect(rect);
                BackendClipPush::Rect
            } else {
                BackendClipPush::None
            }
        }
        BackendClipExecutionKind::ProjectedQuadFallback => {
            if execution.clip_planes.is_some() {
                BackendClipPush::ProjectedQuad
            } else if let Some(rect) = execution.rect {
                let _quad = execution.quad;
                cx.push_clip_rect(rect);
                BackendClipPush::ProjectedQuadFallback
            } else {
                BackendClipPush::None
            }
        }
        BackendClipExecutionKind::MaskFallback => BackendClipPush::MaskFallback,
    }
}

fn push_clip_geometry(cx: &mut Cx2d, geometry: SceneClipGeometry) {
    match geometry {
        SceneClipGeometry::Rect { rect } => cx.push_clip_rect(rect),
    }
}

fn quad_transform_from_local_quad(local_rect: Rect, quad: [DVec2; 4]) -> Mat4f {
    let width = local_rect.size.x;
    let height = local_rect.size.y;
    if width.abs() <= 1e-6 || height.abs() <= 1e-6 {
        return Mat4f::identity();
    }
    let origin = quad[0];
    let x_axis = dvec2((quad[1].x - quad[0].x) / width, (quad[1].y - quad[0].y) / width);
    let y_axis = dvec2((quad[3].x - quad[0].x) / height, (quad[3].y - quad[0].y) / height);
    Mat4f {
        v: [
            x_axis.x as f32, x_axis.y as f32, 0.0, 0.0,
            y_axis.x as f32, y_axis.y as f32, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            origin.x as f32, origin.y as f32, 0.0, 1.0,
        ],
    }
}

fn white_mask_texture(cx: &mut Cx2d) -> Texture {
    let texture = Texture::new_with_format(
        cx.cx,
        TextureFormat::VecBGRAu8_32 {
            data: Some(vec![0xFFFF_FFFF]),
            width: 1,
            height: 1,
            updated: TextureUpdated::Full,
        },
    );
    texture
}
