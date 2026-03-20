use makepad_widgets::*;

use crate::compositor_scene::CompositorScene;
use crate::frame_tree::{FrameId, FrameKey, FrameKind};
use crate::render_plan::RenderPlan;
use crate::paint_items::PaintSource;
use crate::layout_stacking_context::StackingContextSection;

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
    ChildSpatialNode(SpatialNodeId),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SpatialNode {
    pub id: SpatialNodeId,
    pub parent: Option<SpatialNodeId>,
    pub kind: SpatialNodeKind,
    pub owner_node_id: Option<usize>,
    pub world: Mat4f,
    pub world_inverse: Mat4f,
    pub clip_id: SceneClipId,
}

pub(crate) struct SceneSpatialNode<'a> {
    pub key: FrameKey,
    pub kind: SpatialNodeKind,
    pub owner_node_id: Option<usize>,
    pub world: Mat4f,
    pub world_inverse: Mat4f,
    pub clip_id: SceneClipId,
    pub items: Vec<ScenePaintItem<'a>>,
    pub paint_list: Vec<ScenePaintCommand>,
}

pub(crate) struct RenderScene<'a> {
    spatial_nodes: Vec<SpatialNode>,
    root_spatial_node: SpatialNodeId,
    pub(crate) scene_spatial_nodes: Vec<SceneSpatialNode<'a>>,
    pub(crate) clip_nodes: Vec<SceneClipNode>,
    pub(crate) render_plan: RenderPlan,
    compositor_scene: CompositorScene,
}

impl<'a> RenderScene<'a> {
    pub(crate) fn new(
        scene_spatial_nodes: Vec<SceneSpatialNode<'a>>,
        clip_nodes: Vec<SceneClipNode>,
        render_plan: RenderPlan,
        compositor_scene: CompositorScene,
    ) -> Self {
        let mut spatial_nodes = Vec::with_capacity(scene_spatial_nodes.len());
        for (spatial_node_id, spatial_node) in scene_spatial_nodes.iter().enumerate() {
            spatial_nodes.push(SpatialNode {
                id: SpatialNodeId(spatial_node_id),
                parent: parent_spatial_node_id(&scene_spatial_nodes, spatial_node_id),
                kind: spatial_node.kind,
                owner_node_id: spatial_node.owner_node_id,
                world: spatial_node.world,
                world_inverse: spatial_node.world_inverse,
                clip_id: spatial_node.clip_id,
            });
        }
        Self {
            spatial_nodes,
            root_spatial_node: SpatialNodeId(0),
            scene_spatial_nodes,
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

    pub(crate) fn spatial_node_key(&self, id: SpatialNodeId) -> FrameKey {
        self.scene_spatial_nodes[id.0].key
    }

    pub(crate) fn root_frame_id(&self) -> FrameId {
        self.root_spatial_node.0
    }

    pub(crate) fn frame_key(&self, frame_id: FrameId) -> FrameKey {
        self.scene_spatial_nodes[frame_id].key
    }

    pub(crate) fn frame_count(&self) -> usize {
        self.scene_spatial_nodes.len()
    }

    pub(crate) fn frame_items(&self, frame_id: FrameId) -> &[ScenePaintItem<'a>] {
        &self.scene_spatial_nodes[frame_id].items
    }

    pub(crate) fn frame_paint_list(&self, frame_id: FrameId) -> &[ScenePaintCommand] {
        &self.scene_spatial_nodes[frame_id].paint_list
    }

    pub(crate) fn child_frames(&self, frame_id: FrameId) -> impl Iterator<Item = FrameId> + '_ {
        self.frame_paint_list(frame_id)
            .iter()
            .filter_map(|command| match command {
                ScenePaintCommand::ChildSpatialNode(child_frame_id) => Some(child_frame_id.0),
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

    pub(crate) fn frame_surface(&self, frame_id: FrameId) -> Option<usize> {
        self.compositor_scene.frame_surface(frame_id)
    }

    pub(crate) fn frame_parent_surface(&self, frame_id: FrameId) -> Option<usize> {
        self.compositor_scene.frame_parent_surface(frame_id)
    }

    pub(crate) fn with_compositor_scene(mut self, compositor_scene: CompositorScene) -> Self {
        self.compositor_scene = compositor_scene;
        self
    }

    pub(crate) fn frame_participation(&self, frame_id: FrameId) -> crate::render_plan::RenderParticipation {
        self.render_plan.frame_participation(frame_id)
    }

    pub(crate) fn frame_world_transform(&self, frame_id: FrameId) -> Mat4f {
        self.spatial_node(SpatialNodeId(frame_id)).world
    }

    pub(crate) fn frame_world_inverse(&self, frame_id: FrameId) -> Mat4f {
        self.spatial_node(SpatialNodeId(frame_id)).world_inverse
    }

    pub(crate) fn frame_owner_node_id(&self, frame_id: FrameId) -> Option<usize> {
        self.scene_spatial_nodes[frame_id].owner_node_id
    }

    pub(crate) fn frame_clip_id(&self, frame_id: FrameId) -> SceneClipId {
        self.scene_spatial_nodes[frame_id].clip_id
    }
}

pub(crate) fn spatial_kind_from_frame_kind(kind: FrameKind) -> SpatialNodeKind {
    match kind {
        FrameKind::Root => SpatialNodeKind::Root,
        FrameKind::ReferenceFrame => SpatialNodeKind::ReferenceFrame,
        FrameKind::StickyFrame => SpatialNodeKind::Sticky,
        FrameKind::ScrollFrame => SpatialNodeKind::Scroll,
        FrameKind::IFrameRoot => SpatialNodeKind::IFrameRoot,
    }
}

fn parent_spatial_node_id(scene_spatial_nodes: &[SceneSpatialNode<'_>], child_frame_id: usize) -> Option<SpatialNodeId> {
    for (frame_id, frame) in scene_spatial_nodes.iter().enumerate() {
        if frame.paint_list.iter().any(|command| match command {
            ScenePaintCommand::ChildSpatialNode(candidate) => candidate.0 == child_frame_id,
            ScenePaintCommand::Item(_) => false,
        }) {
            return Some(SpatialNodeId(frame_id));
        }
    }
    None
}
