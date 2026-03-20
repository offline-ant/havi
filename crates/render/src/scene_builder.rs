use crate::compositor_scene::CompositorScene;
use crate::frame_tree::{FrameId, FrameKey, FrameKind};
use crate::render_plan::RenderPlan;
use crate::scene::{
    spatial_kind_from_frame_kind, RenderScene, SceneClipId, SceneClipNode, ScenePaintCommand,
    ScenePaintItem, SceneSpatialNode, SpatialNodeId,
};
use crate::paint_items::PaintSource;
use crate::layout_stacking_context::StackingContextSection;
use makepad_widgets::*;

pub(crate) struct RenderSceneBuilder<'a> {
    spatial_nodes: Vec<SceneSpatialNode<'a>>,
    clip_nodes: Vec<SceneClipNode>,
}

impl<'a> RenderSceneBuilder<'a> {
    pub(crate) fn new() -> Self {
        Self {
            spatial_nodes: vec![SceneSpatialNode {
                key: FrameKey::Root,
                kind: spatial_kind_from_frame_kind(FrameKind::Root),
                owner_node_id: None,
                world: Mat4f::identity(),
                world_inverse: Mat4f::identity(),
                clip_id: SceneClipId::INVALID,
                items: Vec::new(),
                paint_list: Vec::new(),
            }],
            clip_nodes: Vec::new(),
        }
    }

    pub(crate) fn root_frame_id(&self) -> FrameId {
        0
    }

    pub(crate) fn child_frame(
        &mut self,
        parent: FrameId,
        key: FrameKey,
        kind: FrameKind,
        owner_node_id: Option<usize>,
        local: Mat4f,
    ) -> FrameId {
        let parent_world = self.spatial_nodes[parent].world;
        let world = Mat4f::mul(&parent_world, &local);
        let world_inverse = world.invert();
        let frame_id = self.spatial_nodes.len();
        self.spatial_nodes.push(SceneSpatialNode {
            key,
            kind: spatial_kind_from_frame_kind(kind),
            owner_node_id,
            world,
            world_inverse,
            clip_id: SceneClipId::INVALID,
            items: Vec::new(),
            paint_list: Vec::new(),
        });
        self.spatial_nodes[parent]
            .paint_list
            .push(ScenePaintCommand::ChildSpatialNode(SpatialNodeId(frame_id)));
        frame_id
    }

    pub(crate) fn rect_clip(
        &mut self,
        parent_frame_id: FrameId,
        parent_clip_id: SceneClipId,
        rect: Rect,
    ) -> SceneClipId {
        let clip_id = SceneClipId(self.clip_nodes.len());
        self.clip_nodes.push(SceneClipNode {
            parent_clip_id,
            parent_spatial_node_id: SpatialNodeId(parent_frame_id),
            rect,
        });
        clip_id
    }

    pub(crate) fn set_frame_clip(&mut self, frame_id: FrameId, clip_id: SceneClipId) {
        self.spatial_nodes[frame_id].clip_id = clip_id;
    }

    pub(crate) fn push_item(
        &mut self,
        frame_id: FrameId,
        source: PaintSource<'a>,
        section: StackingContextSection,
        local_origin: DVec2,
        clip_id: SceneClipId,
    ) {
        let item_index = self.spatial_nodes[frame_id].items.len();
        self.spatial_nodes[frame_id].items.push(ScenePaintItem {
            source,
            section,
            local_origin,
            clip_id,
        });
        self.spatial_nodes[frame_id]
            .paint_list
            .push(ScenePaintCommand::Item(item_index));
    }

    pub(crate) fn build(
        self,
        owner_semantics: std::collections::HashMap<usize, crate::render_plan::NodeRenderSemantics>,
    ) -> RenderScene<'a> {
        let provisional = RenderScene::new(
            self.spatial_nodes,
            self.clip_nodes,
            RenderPlan::default(),
            CompositorScene::default(),
        );
        let render_plan = RenderPlan::build(&provisional, owner_semantics);
        let provisional = RenderScene::new(
            provisional.scene_spatial_nodes,
            provisional.clip_nodes,
            render_plan,
            CompositorScene::default(),
        );
        let compositor_scene = CompositorScene::build(&provisional);
        provisional.with_compositor_scene(compositor_scene)
    }
}
