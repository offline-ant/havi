use makepad_widgets::*;

use crate::clip_tree::{ClipId, ClipNode, ClipTree};
use crate::compositor_scene::CompositorScene;
use crate::frame_tree::{FrameId, FrameKey, FrameKind, FramePaintCommand, FramePaintItem, FrameTree, RenderFrame};
use crate::render_plan::RenderPlan;

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
pub(crate) struct SpatialNode {
    pub id: SpatialNodeId,
    pub parent: Option<SpatialNodeId>,
    pub frame_id: FrameId,
    pub kind: SpatialNodeKind,
    pub owner_node_id: Option<usize>,
    pub world: Mat4f,
    pub world_inverse: Mat4f,
    pub clip_id: ClipId,
}

pub(crate) struct RenderScene<'a> {
    spatial_nodes: Vec<SpatialNode>,
    root_spatial_node: SpatialNodeId,
    pub(crate) frame_tree: FrameTree<'a>,
    pub(crate) clip_tree: ClipTree,
    pub(crate) render_plan: RenderPlan,
    compositor_scene: CompositorScene,
}

impl<'a> RenderScene<'a> {
    pub(crate) fn from_legacy_parts(
        frame_tree: FrameTree<'a>,
        clip_tree: ClipTree,
        render_plan: RenderPlan,
        compositor_scene: CompositorScene,
    ) -> Self {
        let mut spatial_nodes = Vec::with_capacity(frame_tree.frames.len());
        for (frame_id, frame) in frame_tree.frames.iter().enumerate() {
            spatial_nodes.push(SpatialNode {
                id: SpatialNodeId(frame_id),
                parent: parent_spatial_node_id(&frame_tree, frame_id),
                frame_id,
                kind: spatial_kind_from_frame_kind(frame.kind),
                owner_node_id: frame.owner_node_id,
                world: frame.matrix.world,
                world_inverse: frame.matrix.world_inverse,
                clip_id: frame.clip_id,
            });
        }
        Self {
            spatial_nodes,
            root_spatial_node: SpatialNodeId(frame_tree.root_id()),
            frame_tree,
            clip_tree,
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

    pub(crate) fn spatial_node_for_frame(&self, frame_id: FrameId) -> SpatialNodeId {
        SpatialNodeId(frame_id)
    }

    pub(crate) fn root_frame_id(&self) -> FrameId {
        self.frame_tree.root_id()
    }

    pub(crate) fn root_frame(&self) -> &RenderFrame<'a> {
        self.frame(self.root_frame_id())
    }

    pub(crate) fn frame(&self, id: FrameId) -> &RenderFrame<'a> {
        self.frame_tree.frame(id)
    }

    pub(crate) fn frame_key(&self, frame_id: FrameId) -> FrameKey {
        self.frame(frame_id).key
    }

    pub(crate) fn frame_count(&self) -> usize {
        self.frame_tree.frames.len()
    }

    pub(crate) fn frame_items(&self, frame_id: FrameId) -> &[FramePaintItem<'a>] {
        &self.frame(frame_id).items
    }

    pub(crate) fn frame_paint_list(&self, frame_id: FrameId) -> &[FramePaintCommand] {
        &self.frame(frame_id).paint_list
    }

    pub(crate) fn child_frames(&self, frame_id: FrameId) -> impl Iterator<Item = FrameId> + '_ {
        self.frame_paint_list(frame_id)
            .iter()
            .filter_map(|command| match command {
                FramePaintCommand::ChildFrame(child_frame_id) => Some(*child_frame_id),
                FramePaintCommand::Item(_) => None,
            })
    }

    pub(crate) fn clip_node(&self, clip_id: ClipId) -> Option<&ClipNode> {
        if clip_id == ClipId::INVALID {
            None
        } else {
            Some(self.clip_tree.get(clip_id))
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
        self.spatial_node(self.spatial_node_for_frame(frame_id)).world
    }

    pub(crate) fn frame_world_inverse(&self, frame_id: FrameId) -> Mat4f {
        self.spatial_node(self.spatial_node_for_frame(frame_id)).world_inverse
    }
}

fn spatial_kind_from_frame_kind(kind: FrameKind) -> SpatialNodeKind {
    match kind {
        FrameKind::Root => SpatialNodeKind::Root,
        FrameKind::ReferenceFrame => SpatialNodeKind::ReferenceFrame,
        FrameKind::StickyFrame => SpatialNodeKind::Sticky,
        FrameKind::ScrollFrame => SpatialNodeKind::Scroll,
        FrameKind::IFrameRoot => SpatialNodeKind::IFrameRoot,
    }
}

fn parent_spatial_node_id(frame_tree: &FrameTree<'_>, child_frame_id: FrameId) -> Option<SpatialNodeId> {
    for (frame_id, frame) in frame_tree.frames.iter().enumerate() {
        if frame.paint_list.iter().any(|command| match command {
            FramePaintCommand::ChildFrame(candidate) => *candidate == child_frame_id,
            FramePaintCommand::Item(_) => false,
        }) {
            return Some(SpatialNodeId(frame_id));
        }
    }
    None
}
