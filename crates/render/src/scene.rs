use makepad_widgets::*;

use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::PaintSource;
use crate::render_plan::{NodeRenderSemantics, RenderPlan};
use makepad_compositor::{MpBackfaceVisibility, MpTransformStyle};

pub(crate) type PaintContainerId = usize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct SpatialNodeId(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SpatialNodeKind {
    Root,
    ReferenceFrame,
    Scroll,
    Sticky,
    IFrameRoot,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReferenceFrameData {
    pub placement_origin: DVec2,
    pub transform_matrix: Option<Mat4f>,
    pub perspective_matrix: Option<Mat4f>,
    pub transform_style: MpTransformStyle,
    pub flattens_descendants: bool,
    pub backface_visibility: MpBackfaceVisibility,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StickyOffsetConstraints {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StickyOffsetBounds {
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StickyNodeData {
    pub frame_rect: Rect,
    pub margins: StickyOffsetConstraints,
    pub vertical_offset_bounds: StickyOffsetBounds,
    pub horizontal_offset_bounds: StickyOffsetBounds,
    pub containing_block_rect: Rect,
    pub scroll_frame_rect: Rect,
    pub scroll_port_rect: Rect,
    pub nearest_scroll_node_id: Option<SpatialNodeId>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ScrollNodeData {
    pub scroll_offset: DVec2,
    pub scroll_frame_rect: Rect,
    pub sensitivity_x: bool,
    pub sensitivity_y: bool,
    pub external_scroll_node_id: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SpatialNodeSemantics {
    Root,
    ReferenceFrame(ReferenceFrameData),
    Scroll(ScrollNodeData),
    Sticky(StickyNodeData),
    IFrameRoot,
}

impl SpatialNodeSemantics {
    pub(crate) fn kind(self) -> SpatialNodeKind {
        match self {
            SpatialNodeSemantics::Root => SpatialNodeKind::Root,
            SpatialNodeSemantics::ReferenceFrame(_) => SpatialNodeKind::ReferenceFrame,
            SpatialNodeSemantics::Scroll(_) => SpatialNodeKind::Scroll,
            SpatialNodeSemantics::Sticky(_) => SpatialNodeKind::Sticky,
            SpatialNodeSemantics::IFrameRoot => SpatialNodeKind::IFrameRoot,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SceneClipKind {
    Overflow,
    OverflowClip,
    CssClip,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SceneClipGeometry {
    Rect { rect: Rect },
    RoundedRect { rect: Rect, radius: f32 },
    PlaneSet { planes: [Vec4f; 4], count: usize },
    DeferredMask { rect: Rect },
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneClipNode {
    pub parent_clip_id: SceneClipId,
    pub spatial_node_id: SpatialNodeId,
    pub geometry: SceneClipGeometry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct SceneClipId(pub usize);

impl SceneClipId {
    pub(crate) const INVALID: SceneClipId = SceneClipId(usize::MAX);
}

pub(crate) struct ScenePaintItem<'a> {
    pub source: PaintSource<'a>,
    pub section: StackingContextSection,
    pub local_origin: DVec2,
    pub owning_paint_container_id: PaintContainerId,
    pub clip_id: SceneClipId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScenePaintCommand {
    Item(usize),
    ChildPaintContainer(PaintContainerId),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SpatialNode {
    pub parent: Option<SpatialNodeId>,
    pub kind: SpatialNodeKind,
    pub semantics: SpatialNodeSemantics,
    pub world: Mat4f,
    pub world_inverse: Mat4f,
    pub nearest_reference_frame_id: SpatialNodeId,
    pub nearest_scroll_node_id: Option<SpatialNodeId>,
    pub clip_chain_root: SceneClipId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PaintContainerKind {
    Root,
    Normal,
    IFrameRoot { size: DVec2 },
}

pub(crate) struct PaintContainer<'a> {
    pub parent_paint_container_id: Option<PaintContainerId>,
    pub owner_node_id: Option<usize>,
    pub kind: PaintContainerKind,
    pub spatial_node_id: SpatialNodeId,
    pub clip_id: SceneClipId,
    pub items: Vec<ScenePaintItem<'a>>,
    pub paint_list: Vec<ScenePaintCommand>,
}

pub(crate) struct RenderScene<'a> {
    pub(crate) spatial_nodes: Vec<SpatialNode>,
    root_paint_container: PaintContainerId,
    pub(crate) paint_containers: Vec<PaintContainer<'a>>,
    pub(crate) clip_nodes: Vec<SceneClipNode>,
    pub(crate) render_plan: RenderPlan,
}

impl<'a> RenderScene<'a> {
    pub(crate) fn new(
        mut spatial_nodes: Vec<SpatialNode>,
        root_spatial_node: SpatialNodeId,
        paint_containers: Vec<PaintContainer<'a>>,
        root_paint_container: PaintContainerId,
        clip_nodes: Vec<SceneClipNode>,
        render_plan: RenderPlan,
    ) -> Self {
        recompute_spatial_execution(&mut spatial_nodes, root_spatial_node);
        Self {
            spatial_nodes,
            root_paint_container,
            paint_containers,
            clip_nodes,
            render_plan,
        }
    }

    pub(crate) fn spatial_node(&self, id: SpatialNodeId) -> &SpatialNode {
        &self.spatial_nodes[id.0]
    }

    pub(crate) fn root_paint_container_id(&self) -> PaintContainerId {
        self.root_paint_container
    }

    pub(crate) fn frame_items(&self, paint_container_id: PaintContainerId) -> &[ScenePaintItem<'a>] {
        &self.paint_containers[paint_container_id].items
    }

    pub(crate) fn frame_paint_list(&self, paint_container_id: PaintContainerId) -> &[ScenePaintCommand] {
        &self.paint_containers[paint_container_id].paint_list
    }

    pub(crate) fn clip_node(&self, clip_id: SceneClipId) -> Option<&SceneClipNode> {
        if clip_id == SceneClipId::INVALID {
            None
        } else {
            Some(&self.clip_nodes[clip_id.0])
        }
    }

    pub(crate) fn clip_contains_world_point(&self, clip_id: SceneClipId, point_world: DVec2) -> bool {
        let Some(node) = self.clip_node(clip_id) else {
            return true;
        };
        let point_local = transform_point(&self.world_to_spatial_transform(node.spatial_node_id), point_world);
        clip_geometry_contains_point(node.geometry, point_local)
    }

    pub(crate) fn clip_geometry_in_paint_container(
        &self,
        clip_id: SceneClipId,
        paint_container_id: PaintContainerId,
    ) -> Option<SceneClipGeometry> {
        let node = self.clip_node(clip_id)?;
        Some(map_clip_geometry_from_spatial_to_paint_container(
            self,
            node.spatial_node_id,
            paint_container_id,
            node.geometry,
        ))
    }

    pub(crate) fn owner_semantics(&self, node_id: usize) -> Option<NodeRenderSemantics> {
        self.render_plan.owner_semantics(node_id)
    }

    pub(crate) fn paint_container_spatial_node_id(&self, paint_container_id: PaintContainerId) -> SpatialNodeId {
        self.paint_containers[paint_container_id].spatial_node_id
    }

    pub(crate) fn spatial_to_world_transform(&self, spatial_node_id: SpatialNodeId) -> Mat4f {
        self.spatial_node(spatial_node_id).world
    }

    pub(crate) fn world_to_spatial_transform(&self, spatial_node_id: SpatialNodeId) -> Mat4f {
        self.spatial_node(spatial_node_id).world_inverse
    }

    pub(crate) fn frame_world_transform(&self, paint_container_id: PaintContainerId) -> Mat4f {
        self.spatial_to_world_transform(self.paint_container_spatial_node_id(paint_container_id))
    }

    pub(crate) fn frame_world_inverse(&self, paint_container_id: PaintContainerId) -> Mat4f {
        self.world_to_spatial_transform(self.paint_container_spatial_node_id(paint_container_id))
    }

    pub(crate) fn frame_owner_node_id(&self, paint_container_id: PaintContainerId) -> Option<usize> {
        self.paint_containers[paint_container_id].owner_node_id
    }

    pub(crate) fn paint_container_kind(&self, paint_container_id: PaintContainerId) -> PaintContainerKind {
        self.paint_containers[paint_container_id].kind
    }

    pub(crate) fn frame_clip_id(&self, paint_container_id: PaintContainerId) -> SceneClipId {
        self.paint_containers[paint_container_id].clip_id
    }

    pub(crate) fn effective_clip_chain_for_paint_container(
        &self,
        paint_container_id: PaintContainerId,
    ) -> SceneClipId {
        let paint_clip_id = self.frame_clip_id(paint_container_id);
        let spatial_clip_id = self.spatial_node(self.paint_container_spatial_node_id(paint_container_id)).clip_chain_root;
        if paint_clip_id != SceneClipId::INVALID {
            paint_clip_id
        } else {
            spatial_clip_id
        }
    }
}

fn recompute_spatial_execution(spatial_nodes: &mut [SpatialNode], root_spatial_node: SpatialNodeId) {
    if spatial_nodes.is_empty() {
        return;
    }
    let identity = Mat4f::identity();
    let root = root_spatial_node.0;
    spatial_nodes[root].world = identity;
    spatial_nodes[root].world_inverse = identity;
    spatial_nodes[root].nearest_reference_frame_id = root_spatial_node;
    spatial_nodes[root].nearest_scroll_node_id = None;

    for index in 0..spatial_nodes.len() {
        if index == root {
            continue;
        }
        let parent_id = spatial_nodes[index].parent.expect("non-root spatial node must have parent");
        let parent = spatial_nodes[parent_id.0];
        let semantics = spatial_nodes[index].semantics;
        let local = local_execution_transform(semantics);
        let world = Mat4f::mul(&parent.world, &local);
        let world_inverse = world.invert();
        let nearest_reference_frame_id = match semantics {
            SpatialNodeSemantics::ReferenceFrame(_) => SpatialNodeId(index),
            _ => parent.nearest_reference_frame_id,
        };
        let nearest_scroll_node_id = match semantics {
            SpatialNodeSemantics::Scroll(_) => Some(SpatialNodeId(index)),
            _ => parent.nearest_scroll_node_id,
        };
        spatial_nodes[index].kind = semantics.kind();
        spatial_nodes[index].world = world;
        spatial_nodes[index].world_inverse = world_inverse;
        spatial_nodes[index].nearest_reference_frame_id = nearest_reference_frame_id;
        spatial_nodes[index].nearest_scroll_node_id = nearest_scroll_node_id;
    }
}

fn local_execution_transform(semantics: SpatialNodeSemantics) -> Mat4f {
    match semantics {
        SpatialNodeSemantics::Root | SpatialNodeSemantics::IFrameRoot => Mat4f::identity(),
        SpatialNodeSemantics::ReferenceFrame(data) => reference_frame_execution_transform(data),
        SpatialNodeSemantics::Scroll(data) => translation_matrix(
            -(data.scroll_offset.x as f32),
            -(data.scroll_offset.y as f32),
        ),
        SpatialNodeSemantics::Sticky(data) => {
            let offset = sticky_used_offset(data);
            translation_matrix(offset.x as f32, offset.y as f32)
        }
    }
}

fn reference_frame_execution_transform(data: ReferenceFrameData) -> Mat4f {
    let mut transform = translation_matrix(data.placement_origin.x as f32, data.placement_origin.y as f32);
    if let Some(perspective) = data.perspective_matrix {
        transform = Mat4f::mul(&transform, &perspective);
    }
    if let Some(matrix) = data.transform_matrix {
        transform = Mat4f::mul(&transform, &matrix);
    }
    transform
}

fn sticky_used_offset(data: StickyNodeData) -> DVec2 {
    if data.margins.top.is_none()
        && data.margins.right.is_none()
        && data.margins.bottom.is_none()
        && data.margins.left.is_none()
    {
        return dvec2(0.0, 0.0);
    }

    let mut sticky_rect = data.frame_rect;
    let mut sticky_offset = dvec2(0.0, 0.0);

    if let Some(margin) = data.margins.top {
        let top_viewport_edge = data.scroll_port_rect.pos.y + margin as f64;
        if sticky_rect.pos.y < top_viewport_edge {
            sticky_offset.y = top_viewport_edge - sticky_rect.pos.y;
        }
    }

    if sticky_offset.y <= 0.0 {
        if let Some(margin) = data.margins.bottom {
            sticky_rect.pos.y += sticky_offset.y;
            let bottom_viewport_edge =
                data.scroll_port_rect.pos.y + data.scroll_port_rect.size.y - margin as f64;
            let sticky_bottom = sticky_rect.pos.y + sticky_rect.size.y;
            if sticky_bottom > bottom_viewport_edge {
                sticky_offset.y += bottom_viewport_edge - sticky_bottom;
            }
        }
    }

    if let Some(margin) = data.margins.left {
        let left_viewport_edge = data.scroll_port_rect.pos.x + margin as f64;
        if sticky_rect.pos.x < left_viewport_edge {
            sticky_offset.x = left_viewport_edge - sticky_rect.pos.x;
        }
    }

    if sticky_offset.x <= 0.0 {
        if let Some(margin) = data.margins.right {
            sticky_rect.pos.x += sticky_offset.x;
            let right_viewport_edge =
                data.scroll_port_rect.pos.x + data.scroll_port_rect.size.x - margin as f64;
            let sticky_right = sticky_rect.pos.x + sticky_rect.size.x;
            if sticky_right > right_viewport_edge {
                sticky_offset.x += right_viewport_edge - sticky_right;
            }
        }
    }

    sticky_offset.y = sticky_offset
        .y
        .max(data.vertical_offset_bounds.min as f64)
        .min(data.vertical_offset_bounds.max as f64);
    sticky_offset.x = sticky_offset
        .x
        .max(data.horizontal_offset_bounds.min as f64)
        .min(data.horizontal_offset_bounds.max as f64);

    let frame_left = data.frame_rect.pos.x;
    let frame_top = data.frame_rect.pos.y;
    let frame_right = data.frame_rect.pos.x + data.frame_rect.size.x;
    let frame_bottom = data.frame_rect.pos.y + data.frame_rect.size.y;
    let cb_left = data.containing_block_rect.pos.x;
    let cb_top = data.containing_block_rect.pos.y;
    let cb_right = data.containing_block_rect.pos.x + data.containing_block_rect.size.x;
    let cb_bottom = data.containing_block_rect.pos.y + data.containing_block_rect.size.y;
    sticky_offset.x = sticky_offset.x.max(cb_left - frame_left).min(cb_right - frame_right);
    sticky_offset.y = sticky_offset.y.max(cb_top - frame_top).min(cb_bottom - frame_bottom);

    dvec2(sticky_offset.x, sticky_offset.y)
}

fn clip_geometry_contains_point(geometry: SceneClipGeometry, point: DVec2) -> bool {
    match geometry {
        SceneClipGeometry::Rect { rect } | SceneClipGeometry::DeferredMask { rect } => {
            point_in_rect(point, rect)
        }
        SceneClipGeometry::RoundedRect { rect, radius } => rounded_rect_contains_point(rect, radius, point),
        SceneClipGeometry::PlaneSet { planes, count } => planes[..count].iter().all(|plane| {
            plane.x as f64 * point.x + plane.y as f64 * point.y + plane.w as f64 >= 0.0
        }),
    }
}

fn map_clip_geometry_from_spatial_to_paint_container(
    scene: &RenderScene<'_>,
    from_spatial_node_id: SpatialNodeId,
    to_paint_container_id: PaintContainerId,
    geometry: SceneClipGeometry,
) -> SceneClipGeometry {
    let map = Mat4f::mul(
        &scene.frame_world_inverse(to_paint_container_id),
        &scene.spatial_to_world_transform(from_spatial_node_id),
    );
    match geometry {
        SceneClipGeometry::Rect { rect } => map_rect_clip_geometry(&map, rect),
        SceneClipGeometry::RoundedRect { rect, radius } => {
            if transform_is_axis_aligned_2d(&map) {
                SceneClipGeometry::RoundedRect {
                    rect: transform_rect(&map, rect),
                    radius: mapped_axis_aligned_radius(&map, radius),
                }
            } else {
                SceneClipGeometry::DeferredMask {
                    rect: transform_rect(&map, rect),
                }
            }
        }
        SceneClipGeometry::PlaneSet { planes, count } => {
            let plane_transform = map.invert().transpose();
            let mut mapped = [vec4(0.0, 0.0, 0.0, 0.0); 4];
            for index in 0..count {
                mapped[index] = plane_transform.transform_vec4(planes[index]);
            }
            SceneClipGeometry::PlaneSet {
                planes: mapped,
                count,
            }
        }
        SceneClipGeometry::DeferredMask { rect } => SceneClipGeometry::DeferredMask {
            rect: transform_rect(&map, rect),
        },
    }
}

fn map_rect_clip_geometry(map: &Mat4f, rect: Rect) -> SceneClipGeometry {
    if transform_is_axis_aligned_2d(map) {
        return SceneClipGeometry::Rect {
            rect: transform_rect(map, rect),
        };
    }
    let quad = transform_rect_quad(map, rect);
    match quad_as_plane_set(quad) {
        Some((planes, count)) => SceneClipGeometry::PlaneSet { planes, count },
        None => SceneClipGeometry::DeferredMask {
            rect: transform_rect(map, rect),
        },
    }
}

fn transform_rect_quad(matrix: &Mat4f, rect: Rect) -> [DVec2; 4] {
    [
        transform_point(matrix, rect.pos),
        transform_point(matrix, dvec2(rect.pos.x + rect.size.x, rect.pos.y)),
        transform_point(matrix, rect.pos + rect.size),
        transform_point(matrix, dvec2(rect.pos.x, rect.pos.y + rect.size.y)),
    ]
}

fn quad_as_plane_set(quad: [DVec2; 4]) -> Option<([Vec4f; 4], usize)> {
    let mut planes = [vec4(0.0, 0.0, 0.0, 0.0); 4];
    for index in 0..4 {
        let from = quad[index];
        let to = quad[(index + 1) % 4];
        let edge = dvec2(to.x - from.x, to.y - from.y);
        let length = (edge.x * edge.x + edge.y * edge.y).sqrt();
        if length <= 1e-6 {
            return None;
        }
        let normal = dvec2(-edge.y / length, edge.x / length);
        let distance = -(normal.x * from.x + normal.y * from.y);
        planes[index] = vec4(normal.x as f32, normal.y as f32, 0.0, distance as f32);
    }
    Some((planes, 4))
}

fn point_in_rect(point: DVec2, rect: Rect) -> bool {
    point.x >= rect.pos.x
        && point.x < rect.pos.x + rect.size.x
        && point.y >= rect.pos.y
        && point.y < rect.pos.y + rect.size.y
}

fn rounded_rect_contains_point(rect: Rect, radius: f32, point: DVec2) -> bool {
    if !point_in_rect(point, rect) {
        return false;
    }
    let radius = radius.max(0.0) as f64;
    if radius <= 0.0 {
        return true;
    }
    let clamped_radius = radius.min(rect.size.x * 0.5).min(rect.size.y * 0.5);
    let inner = Rect {
        pos: dvec2(rect.pos.x + clamped_radius, rect.pos.y + clamped_radius),
        size: dvec2(
            (rect.size.x - clamped_radius * 2.0).max(0.0),
            (rect.size.y - clamped_radius * 2.0).max(0.0),
        ),
    };
    if point_in_rect(
        point,
        Rect {
            pos: dvec2(rect.pos.x + clamped_radius, rect.pos.y),
            size: dvec2((rect.size.x - clamped_radius * 2.0).max(0.0), rect.size.y),
        },
    ) || point_in_rect(
        point,
        Rect {
            pos: dvec2(rect.pos.x, rect.pos.y + clamped_radius),
            size: dvec2(rect.size.x, (rect.size.y - clamped_radius * 2.0).max(0.0)),
        },
    ) || point_in_rect(point, inner)
    {
        return true;
    }
    let corners = [
        dvec2(rect.pos.x + clamped_radius, rect.pos.y + clamped_radius),
        dvec2(rect.pos.x + rect.size.x - clamped_radius, rect.pos.y + clamped_radius),
        dvec2(
            rect.pos.x + rect.size.x - clamped_radius,
            rect.pos.y + rect.size.y - clamped_radius,
        ),
        dvec2(rect.pos.x + clamped_radius, rect.pos.y + rect.size.y - clamped_radius),
    ];
    corners.iter().any(|center| {
        let dx = point.x - center.x;
        let dy = point.y - center.y;
        dx * dx + dy * dy <= clamped_radius * clamped_radius
    })
}

fn transform_is_axis_aligned_2d(matrix: &Mat4f) -> bool {
    matrix.v[1].abs() <= 1e-6
        && matrix.v[2].abs() <= 1e-6
        && matrix.v[3].abs() <= 1e-6
        && matrix.v[4].abs() <= 1e-6
        && matrix.v[6].abs() <= 1e-6
        && matrix.v[7].abs() <= 1e-6
        && matrix.v[8].abs() <= 1e-6
        && matrix.v[9].abs() <= 1e-6
        && (matrix.v[10] - 1.0).abs() <= 1e-6
        && matrix.v[11].abs() <= 1e-6
        && matrix.v[14].abs() <= 1e-6
        && (matrix.v[15] - 1.0).abs() <= 1e-6
}

fn mapped_axis_aligned_radius(matrix: &Mat4f, radius: f32) -> f32 {
    let sx = matrix.v[0].abs();
    let sy = matrix.v[5].abs();
    radius * sx.max(sy)
}

fn transform_point(matrix: &Mat4f, point: DVec2) -> DVec2 {
    let mapped = matrix.transform_vec4(vec4f(point.x as f32, point.y as f32, 0.0, 1.0));
    if mapped.w.abs() > 1e-6 {
        dvec2((mapped.x / mapped.w) as f64, (mapped.y / mapped.w) as f64)
    } else {
        dvec2(mapped.x as f64, mapped.y as f64)
    }
}

fn transform_rect(matrix: &Mat4f, rect: Rect) -> Rect {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect {
            pos: dvec2(x, y),
            size: dvec2(w, h),
        }
    }

    fn scene_with_child_transform(transform: Mat4f) -> RenderScene<'static> {
        let root_spatial_node = SpatialNodeId(0);
        let child_spatial_node = SpatialNodeId(1);
        RenderScene::new(
            vec![
                SpatialNode {
                    parent: None,
                    kind: SpatialNodeKind::Root,
                    semantics: SpatialNodeSemantics::Root,
                    world: Mat4f::identity(),
                    world_inverse: Mat4f::identity(),
                    nearest_reference_frame_id: root_spatial_node,
                    nearest_scroll_node_id: None,
                    clip_chain_root: SceneClipId::INVALID,
                },
                SpatialNode {
                    parent: Some(root_spatial_node),
                    kind: SpatialNodeKind::ReferenceFrame,
                    semantics: SpatialNodeSemantics::ReferenceFrame(ReferenceFrameData {
                        placement_origin: dvec2(0.0, 0.0),
                        transform_matrix: Some(transform),
                        perspective_matrix: None,
                        transform_style: MpTransformStyle::Flat,
                        flattens_descendants: true,
                        backface_visibility: MpBackfaceVisibility::Visible,
                    }),
                    world: Mat4f::identity(),
                    world_inverse: Mat4f::identity(),
                    nearest_reference_frame_id: child_spatial_node,
                    nearest_scroll_node_id: None,
                    clip_chain_root: SceneClipId::INVALID,
                },
            ],
            root_spatial_node,
            vec![
                PaintContainer {
                    owner_node_id: None,
                    kind: PaintContainerKind::Root,
                    spatial_node_id: root_spatial_node,
                    clip_id: SceneClipId::INVALID,
                    items: Vec::new(),
                    paint_list: Vec::new(),
                },
                PaintContainer {
                    owner_node_id: Some(1),
                    kind: PaintContainerKind::Normal,
                    spatial_node_id: child_spatial_node,
                    clip_id: SceneClipId::INVALID,
                    items: Vec::new(),
                    paint_list: Vec::new(),
                },
            ],
            0,
            vec![SceneClipNode {
                parent_clip_id: SceneClipId::INVALID,
                spatial_node_id: child_spatial_node,
                geometry: SceneClipGeometry::Rect {
                    rect: rect(0.0, 0.0, 40.0, 20.0),
                },
            }],
            RenderPlan::build(HashMap::new()),
        )
    }

    #[test]
    fn rotated_rect_clip_maps_to_plane_set() {
        let scene = scene_with_child_transform(Mat4f::rotation(vec3(0.0, 0.0, 0.4)));
        let geometry = scene
            .clip_geometry_in_paint_container(SceneClipId(0), 0)
            .unwrap();
        match geometry {
            SceneClipGeometry::PlaneSet { count, .. } => assert_eq!(count, 4),
            other => panic!("expected plane-set clip, got {other:?}"),
        }
    }

    #[test]
    fn rounded_rect_contains_point_uses_corner_radius() {
        let geometry = SceneClipGeometry::RoundedRect {
            rect: rect(0.0, 0.0, 10.0, 10.0),
            radius: 5.0,
        };
        assert!(clip_geometry_contains_point(geometry, dvec2(3.0, 3.0)));
        assert!(!clip_geometry_contains_point(geometry, dvec2(1.0, 1.0)));
    }
}
