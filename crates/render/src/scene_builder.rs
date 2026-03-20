use crate::compositor_scene::CompositorScene;
use crate::frame_tree::{FrameId, FrameKey, FrameKind};
use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::PaintSource;
use crate::render_plan::RenderPlan;
use crate::scene::{
    PaintContainer, PaintContainerId, RenderScene, SceneClipId, SceneClipNode, ScenePaintCommand,
    ScenePaintItem, SpatialNode, SpatialNodeId, SpatialNodeKind,
};
use makepad_widgets::*;

pub(crate) struct RenderSceneBuilder<'a> {
    spatial_nodes: Vec<SpatialNode>,
    paint_containers: Vec<PaintContainer<'a>>,
    clip_nodes: Vec<SceneClipNode>,
    root_spatial_node_id: SpatialNodeId,
    root_paint_container_id: PaintContainerId,
}

impl<'a> RenderSceneBuilder<'a> {
    pub(crate) fn new() -> Self {
        let root_spatial_node_id = SpatialNodeId(0);
        let root_paint_container_id = 0;
        Self {
            spatial_nodes: vec![SpatialNode {
                id: root_spatial_node_id,
                parent: None,
                kind: SpatialNodeKind::Root,
                owner_node_id: None,
                world: Mat4f::identity(),
                world_inverse: Mat4f::identity(),
            }],
            paint_containers: vec![PaintContainer {
                key: FrameKey::Root,
                owner_node_id: None,
                spatial_node_id: root_spatial_node_id,
                clip_id: SceneClipId::INVALID,
                items: Vec::new(),
                paint_list: Vec::new(),
            }],
            clip_nodes: Vec::new(),
            root_spatial_node_id,
            root_paint_container_id,
        }
    }

    pub(crate) fn root_frame_id(&self) -> FrameId {
        self.root_paint_container_id
    }

    pub(crate) fn child_frame(
        &mut self,
        parent_paint_container_id: PaintContainerId,
        key: FrameKey,
        kind: FrameKind,
        owner_node_id: Option<usize>,
        local: Mat4f,
    ) -> PaintContainerId {
        let parent_spatial_node_id = self.paint_containers[parent_paint_container_id].spatial_node_id;
        let parent_world = self.spatial_nodes[parent_spatial_node_id.0].world;
        let world = Mat4f::mul(&parent_world, &local);
        let world_inverse = world.invert();

        let spatial_node_id = SpatialNodeId(self.spatial_nodes.len());
        self.spatial_nodes.push(SpatialNode {
            id: spatial_node_id,
            parent: Some(parent_spatial_node_id),
            kind: spatial_kind_from_frame_kind(kind),
            owner_node_id,
            world,
            world_inverse,
        });

        let paint_container_id = self.paint_containers.len();
        self.paint_containers.push(PaintContainer {
            key,
            owner_node_id,
            spatial_node_id,
            clip_id: SceneClipId::INVALID,
            items: Vec::new(),
            paint_list: Vec::new(),
        });
        self.paint_containers[parent_paint_container_id]
            .paint_list
            .push(ScenePaintCommand::ChildPaintContainer(paint_container_id));
        paint_container_id
    }

    pub(crate) fn rect_clip(
        &mut self,
        parent_paint_container_id: PaintContainerId,
        parent_clip_id: SceneClipId,
        rect: Rect,
    ) -> SceneClipId {
        let clip_id = SceneClipId(self.clip_nodes.len());
        self.clip_nodes.push(SceneClipNode {
            parent_clip_id,
            parent_spatial_node_id: self.paint_containers[parent_paint_container_id].spatial_node_id,
            rect,
        });
        clip_id
    }

    pub(crate) fn set_frame_clip(&mut self, paint_container_id: PaintContainerId, clip_id: SceneClipId) {
        self.paint_containers[paint_container_id].clip_id = clip_id;
    }

    pub(crate) fn push_item(
        &mut self,
        paint_container_id: PaintContainerId,
        source: PaintSource<'a>,
        section: StackingContextSection,
        local_origin: DVec2,
        clip_id: SceneClipId,
    ) {
        let item_index = self.paint_containers[paint_container_id].items.len();
        self.paint_containers[paint_container_id].items.push(ScenePaintItem {
            source,
            section,
            local_origin,
            clip_id,
        });
        self.paint_containers[paint_container_id]
            .paint_list
            .push(ScenePaintCommand::Item(item_index));
    }

    pub(crate) fn build(
        self,
        owner_semantics: std::collections::HashMap<usize, crate::render_plan::NodeRenderSemantics>,
    ) -> RenderScene<'a> {
        let provisional = RenderScene::new(
            self.spatial_nodes,
            self.root_spatial_node_id,
            self.paint_containers,
            self.root_paint_container_id,
            self.clip_nodes,
            RenderPlan::default(),
            CompositorScene::default(),
        );
        let render_plan = RenderPlan::build(&provisional, owner_semantics);
        let root_spatial_node_id = provisional.root_spatial_node();
        let root_paint_container_id = provisional.root_paint_container_id();
        let provisional = RenderScene::new(
            provisional.spatial_nodes.clone(),
            root_spatial_node_id,
            provisional.paint_containers,
            root_paint_container_id,
            provisional.clip_nodes,
            render_plan,
            CompositorScene::default(),
        );
        let compositor_scene = CompositorScene::build(&provisional);
        provisional.with_compositor_scene(compositor_scene)
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
