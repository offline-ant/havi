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
pub(crate) struct SceneClipNode {
    pub parent_clip_id: SceneClipId,
    pub parent_spatial_node_id: SpatialNodeId,
    pub rect: Rect,
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
    pub owner_node_id: Option<usize>,
    pub world: Mat4f,
    pub world_inverse: Mat4f,
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
        spatial_nodes: Vec<SpatialNode>,
        root_spatial_node: SpatialNodeId,
        paint_containers: Vec<PaintContainer<'a>>,
        root_paint_container: PaintContainerId,
        clip_nodes: Vec<SceneClipNode>,
        render_plan: RenderPlan,
        compositor_scene: CompositorScene,
    ) -> Self {
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

    pub(crate) fn child_frames(&self, paint_container_id: PaintContainerId) -> impl Iterator<Item = PaintContainerId> + '_ {
        self.frame_paint_list(paint_container_id)
            .iter()
            .filter_map(|command| match command {
                ScenePaintCommand::ChildPaintContainer(child_paint_container_id) => Some(*child_paint_container_id),
                ScenePaintCommand::Item(_) => None,
            })
    }

    pub(crate) fn clip_node(&self, clip_id: SceneClipId) -> Option<&SceneClipNode> {
        if clip_id == SceneClipId::INVALID {
            None
        } else {
            Some(&self.clip_nodes[clip_id.0])
        }
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

    pub(crate) fn frame_world_transform(&self, paint_container_id: PaintContainerId) -> Mat4f {
        self.spatial_node(self.paint_container_spatial_node_id(paint_container_id)).world
    }

    pub(crate) fn frame_world_inverse(&self, paint_container_id: PaintContainerId) -> Mat4f {
        self.spatial_node(self.paint_container_spatial_node_id(paint_container_id)).world_inverse
    }

    pub(crate) fn frame_owner_node_id(&self, paint_container_id: PaintContainerId) -> Option<usize> {
        self.paint_containers[paint_container_id].owner_node_id
    }

    pub(crate) fn frame_clip_id(&self, paint_container_id: PaintContainerId) -> SceneClipId {
        self.paint_containers[paint_container_id].clip_id
    }
}
