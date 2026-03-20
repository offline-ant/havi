use makepad_widgets::*;

use crate::compositor_scene::CompositorScene;
use crate::frame_tree::FrameKey;
use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::PaintSource;
use crate::render_plan::RenderPlan;

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
    pub origin: DVec2,
    pub transform_matrix: Option<Mat4f>,
    pub perspective_matrix: Option<Mat4f>,
    pub has_transform: bool,
    pub has_perspective: bool,
    pub preserves_3d: bool,
    pub anchors_content: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StickyOffsetConstraints {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StickyNodeData {
    pub constraint_rect: Rect,
    pub frame_rect: Rect,
    pub containing_block_rect: Rect,
    pub scroll_container_rect: Rect,
    pub nearest_scroll_node_id: Option<SpatialNodeId>,
    pub offsets: StickyOffsetConstraints,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ScrollNodeData {
    pub scroll_offset: DVec2,
    pub scroll_frame_rect: Rect,
    pub content_rect: Rect,
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
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SceneClipGeometry {
    Rect { rect: Rect },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BackendClipExecutionKind {
    DirectRect,
    ProjectedQuadFallback,
    MaskFallback,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BackendClipExecution {
    pub kind: BackendClipExecutionKind,
    pub rect: Option<Rect>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneClipNode {
    pub parent_clip_id: SceneClipId,
    pub spatial_node_id: SpatialNodeId,
    pub geometry: SceneClipGeometry,
    pub scroll_node_id: Option<SpatialNodeId>,
    pub overflow_root_spatial_node_id: Option<SpatialNodeId>,
    pub reference_frame_id: SpatialNodeId,
    pub kind: SceneClipKind,
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
    pub clip_id: SceneClipId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScenePaintCommand {
    Item(usize),
    ChildPaintContainer(PaintContainerId),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SpatialNode {
    pub id: SpatialNodeId,
    pub parent: Option<SpatialNodeId>,
    pub kind: SpatialNodeKind,
    pub semantics: SpatialNodeSemantics,
    pub owner_node_id: Option<usize>,
    pub world: Mat4f,
    pub world_inverse: Mat4f,
    pub nearest_reference_frame_id: SpatialNodeId,
    pub nearest_scroll_node_id: Option<SpatialNodeId>,
    pub clip_chain_root: SceneClipId,
}

pub(crate) struct PaintContainer<'a> {
    pub key: FrameKey,
    pub owner_node_id: Option<usize>,
    pub spatial_node_id: SpatialNodeId,
    pub clip_id: SceneClipId,
    pub items: Vec<ScenePaintItem<'a>>,
    pub paint_list: Vec<ScenePaintCommand>,
}

pub(crate) struct RenderScene<'a> {
    pub(crate) spatial_nodes: Vec<SpatialNode>,
    root_spatial_node: SpatialNodeId,
    root_paint_container: PaintContainerId,
    pub(crate) paint_containers: Vec<PaintContainer<'a>>,
    pub(crate) clip_nodes: Vec<SceneClipNode>,
    pub(crate) render_plan: RenderPlan,
    compositor_scene: CompositorScene,
}

impl<'a> RenderScene<'a> {
    pub(crate) fn new(
        mut spatial_nodes: Vec<SpatialNode>,
        root_spatial_node: SpatialNodeId,
        paint_containers: Vec<PaintContainer<'a>>,
        root_paint_container: PaintContainerId,
        clip_nodes: Vec<SceneClipNode>,
        render_plan: RenderPlan,
        compositor_scene: CompositorScene,
    ) -> Self {
        recompute_spatial_execution(&mut spatial_nodes, root_spatial_node);
        Self {
            spatial_nodes,
            root_spatial_node,
            root_paint_container,
            paint_containers,
            clip_nodes,
            render_plan,
            compositor_scene,
        }
    }

    pub(crate) fn root_spatial_node(&self) -> SpatialNodeId {
        self.root_spatial_node
    }

    pub(crate) fn spatial_node(&self, id: SpatialNodeId) -> &SpatialNode {
        &self.spatial_nodes[id.0]
    }

    pub(crate) fn root_paint_container_id(&self) -> PaintContainerId {
        self.root_paint_container
    }

    pub(crate) fn frame_key(&self, paint_container_id: PaintContainerId) -> FrameKey {
        self.paint_containers[paint_container_id].key
    }

    pub(crate) fn frame_count(&self) -> usize {
        self.paint_containers.len()
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

    pub(crate) fn clip_chain_has_reference_frame_effects(&self, clip_id: SceneClipId) -> bool {
        let mut current = clip_id;
        while current != SceneClipId::INVALID {
            let node = self.clip_node(current).unwrap();
            let reference_frame = self.spatial_node(node.reference_frame_id);
            if let SpatialNodeSemantics::ReferenceFrame(data) = reference_frame.semantics {
                if data.has_perspective || data.preserves_3d {
                    return true;
                }
            }
            current = node.parent_clip_id;
        }
        false
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

    pub(crate) fn backend_clip_execution(
        &self,
        clip_id: SceneClipId,
        paint_container_id: PaintContainerId,
    ) -> Option<BackendClipExecution> {
        let node = self.clip_node(clip_id)?;
        let geometry = self.clip_geometry_in_paint_container(clip_id, paint_container_id)?;
        let reference_frame = self.spatial_node(node.reference_frame_id);
        let kind = match reference_frame.semantics {
            SpatialNodeSemantics::ReferenceFrame(data) if data.has_perspective || data.preserves_3d => {
                BackendClipExecutionKind::ProjectedQuadFallback
            }
            _ => match geometry {
                SceneClipGeometry::Rect { .. } => BackendClipExecutionKind::DirectRect,
            },
        };
        let rect = match geometry {
            SceneClipGeometry::Rect { rect } => Some(rect),
        };
        Some(BackendClipExecution { kind, rect })
    }

    pub(crate) fn frame_surface(&self, paint_container_id: PaintContainerId) -> Option<usize> {
        self.compositor_scene.frame_surface(paint_container_id)
    }

    pub(crate) fn frame_parent_surface(&self, paint_container_id: PaintContainerId) -> Option<usize> {
        self.compositor_scene.frame_parent_surface(paint_container_id)
    }

    pub(crate) fn with_compositor_scene(mut self, compositor_scene: CompositorScene) -> Self {
        self.compositor_scene = compositor_scene;
        self
    }

    pub(crate) fn frame_participation(&self, paint_container_id: PaintContainerId) -> crate::render_plan::RenderParticipation {
        self.render_plan.frame_participation(paint_container_id)
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
    let mut transform = Mat4f::identity();
    if let Some(perspective) = data.perspective_matrix {
        transform = Mat4f::mul(&transform, &perspective);
    }
    if let Some(matrix) = data.transform_matrix {
        transform = Mat4f::mul(&transform, &matrix);
    }
    transform
}

fn sticky_used_offset(data: StickyNodeData) -> DVec2 {
    let mut dx = 0.0;
    let mut dy = 0.0;

    let frame_left = data.frame_rect.pos.x;
    let frame_top = data.frame_rect.pos.y;
    let frame_right = data.frame_rect.pos.x + data.frame_rect.size.x;
    let frame_bottom = data.frame_rect.pos.y + data.frame_rect.size.y;
    let cb_left = data.containing_block_rect.pos.x;
    let cb_top = data.containing_block_rect.pos.y;
    let cb_right = data.containing_block_rect.pos.x + data.containing_block_rect.size.x;
    let cb_bottom = data.containing_block_rect.pos.y + data.containing_block_rect.size.y;

    let scroll_rect = data.scroll_container_rect;
    let scroll_left = scroll_rect.pos.x;
    let scroll_top = scroll_rect.pos.y;
    let scroll_right = scroll_rect.pos.x + scroll_rect.size.x;
    let scroll_bottom = scroll_rect.pos.y + scroll_rect.size.y;

    if let Some(left) = data.offsets.left {
        dx = (scroll_left + left as f64) - frame_left;
        dx = dx.max(cb_left - frame_left);
        dx = dx.min(cb_right - frame_right);
    } else if let Some(right) = data.offsets.right {
        dx = (scroll_right - right as f64) - frame_right;
        dx = dx.max(cb_left - frame_left);
        dx = dx.min(cb_right - frame_right);
    }

    if let Some(top) = data.offsets.top {
        dy = (scroll_top + top as f64) - frame_top;
        dy = dy.max(cb_top - frame_top);
        dy = dy.min(cb_bottom - frame_bottom);
    } else if let Some(bottom) = data.offsets.bottom {
        dy = (scroll_bottom - bottom as f64) - frame_bottom;
        dy = dy.max(cb_top - frame_top);
        dy = dy.min(cb_bottom - frame_bottom);
    }

    dvec2(dx, dy)
}

fn clip_geometry_contains_point(geometry: SceneClipGeometry, point: DVec2) -> bool {
    match geometry {
        SceneClipGeometry::Rect { rect } => {
            point.x >= rect.pos.x
                && point.x < rect.pos.x + rect.size.x
                && point.y >= rect.pos.y
                && point.y < rect.pos.y + rect.size.y
        }
    }
}

fn map_clip_geometry_from_spatial_to_paint_container(
    scene: &RenderScene<'_>,
    from_spatial_node_id: SpatialNodeId,
    to_paint_container_id: PaintContainerId,
    geometry: SceneClipGeometry,
) -> SceneClipGeometry {
    match geometry {
        SceneClipGeometry::Rect { rect } => {
            let world_rect = transform_rect(&scene.spatial_to_world_transform(from_spatial_node_id), rect);
            let mapped = transform_rect(&scene.frame_world_inverse(to_paint_container_id), world_rect);
            SceneClipGeometry::Rect { rect: mapped }
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
