use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::PaintSource;
use crate::render_plan::RenderPlan;
use crate::scene::{
    PaintContainer, PaintContainerId, PaintContainerKind, ReferenceFrameData, RenderScene,
    SceneClipGeometry, SceneClipId, SceneClipKind, SceneClipNode, ScenePaintCommand,
    ScenePaintItem, ScrollNodeData, SpatialNode, SpatialNodeId, SpatialNodeSemantics,
    StickyNodeData,
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
                parent: None,
                kind: crate::scene::SpatialNodeKind::Root,
                semantics: SpatialNodeSemantics::Root,
                world: Mat4f::identity(),
                world_inverse: Mat4f::identity(),
                nearest_reference_frame_id: root_spatial_node_id,
                nearest_scroll_node_id: None,
                clip_chain_root: SceneClipId::INVALID,
            }],
            paint_containers: vec![PaintContainer {
                parent_paint_container_id: None,
                owner_node_id: None,
                kind: PaintContainerKind::Root,
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

    pub(crate) fn root_paint_container_id(&self) -> PaintContainerId {
        self.root_paint_container_id
    }

    pub(crate) fn paint_container_spatial_node_id(
        &self,
        paint_container_id: PaintContainerId,
    ) -> SpatialNodeId {
        self.paint_containers[paint_container_id].spatial_node_id
    }

    pub(crate) fn child_spatial_node(
        &mut self,
        parent_spatial_node_id: SpatialNodeId,
        semantics: SpatialNodeSemantics,
        _owner_node_id: Option<usize>,
    ) -> SpatialNodeId {
        let parent = self.spatial_nodes[parent_spatial_node_id.0];
        let spatial_node_id = SpatialNodeId(self.spatial_nodes.len());
        let nearest_reference_frame_id = match semantics {
            SpatialNodeSemantics::ReferenceFrame(_) => spatial_node_id,
            _ => parent.nearest_reference_frame_id,
        };
        let nearest_scroll_node_id = match semantics {
            SpatialNodeSemantics::Scroll(_) => Some(spatial_node_id),
            _ => parent.nearest_scroll_node_id,
        };
        self.spatial_nodes.push(SpatialNode {
            parent: Some(parent_spatial_node_id),
            kind: semantics.kind(),
            semantics,
            world: Mat4f::identity(),
            world_inverse: Mat4f::identity(),
            nearest_reference_frame_id,
            nearest_scroll_node_id,
            clip_chain_root: parent.clip_chain_root,
        });
        spatial_node_id
    }

    pub(crate) fn child_reference_frame(
        &mut self,
        parent_spatial_node_id: SpatialNodeId,
        owner_node_id: Option<usize>,
        data: ReferenceFrameData,
    ) -> SpatialNodeId {
        self.child_spatial_node(
            parent_spatial_node_id,
            SpatialNodeSemantics::ReferenceFrame(data),
            owner_node_id,
        )
    }

    pub(crate) fn child_sticky_node(
        &mut self,
        parent_spatial_node_id: SpatialNodeId,
        owner_node_id: Option<usize>,
        mut data: StickyNodeData,
    ) -> SpatialNodeId {
        data.nearest_scroll_node_id =
            self.spatial_nodes[parent_spatial_node_id.0].nearest_scroll_node_id;
        if let Some(scroll_node_id) = data.nearest_scroll_node_id {
            if let SpatialNodeSemantics::Scroll(scroll) = self.spatial_nodes[scroll_node_id.0].semantics {
                data.scroll_port_rect = Rect {
                    pos: dvec2(
                        data.scroll_frame_rect.pos.x - scroll.scroll_offset.x,
                        data.scroll_frame_rect.pos.y - scroll.scroll_offset.y,
                    ),
                    size: data.scroll_frame_rect.size,
                };
            }
        }
        self.child_spatial_node(
            parent_spatial_node_id,
            SpatialNodeSemantics::Sticky(data),
            owner_node_id,
        )
    }

    pub(crate) fn child_scroll_node(
        &mut self,
        parent_spatial_node_id: SpatialNodeId,
        owner_node_id: Option<usize>,
        data: ScrollNodeData,
    ) -> SpatialNodeId {
        self.child_spatial_node(
            parent_spatial_node_id,
            SpatialNodeSemantics::Scroll(data),
            owner_node_id,
        )
    }

    pub(crate) fn child_iframe_root_node(
        &mut self,
        parent_spatial_node_id: SpatialNodeId,
        owner_node_id: Option<usize>,
    ) -> SpatialNodeId {
        self.child_spatial_node(
            parent_spatial_node_id,
            SpatialNodeSemantics::IFrameRoot,
            owner_node_id,
        )
    }

    pub(crate) fn child_paint_container(
        &mut self,
        parent_paint_container_id: PaintContainerId,
        spatial_node_id: SpatialNodeId,
        owner_node_id: Option<usize>,
    ) -> PaintContainerId {
        self.child_paint_container_with_kind(
            parent_paint_container_id,
            spatial_node_id,
            owner_node_id,
            PaintContainerKind::Normal,
        )
    }

    pub(crate) fn child_paint_container_with_kind(
        &mut self,
        parent_paint_container_id: PaintContainerId,
        spatial_node_id: SpatialNodeId,
        owner_node_id: Option<usize>,
        kind: PaintContainerKind,
    ) -> PaintContainerId {
        let paint_container_id = self.paint_containers.len();
        self.paint_containers.push(PaintContainer {
            parent_paint_container_id: Some(parent_paint_container_id),
            owner_node_id,
            kind,
            spatial_node_id,
            clip_id: self.spatial_nodes[spatial_node_id.0].clip_chain_root,
            items: Vec::new(),
            paint_list: Vec::new(),
        });
        paint_container_id
    }

    pub(crate) fn append_child_paint_container(
        &mut self,
        parent_paint_container_id: PaintContainerId,
        child_paint_container_id: PaintContainerId,
    ) {
        if child_paint_container_id == parent_paint_container_id {
            return;
        }
        let paint_list = &mut self.paint_containers[parent_paint_container_id].paint_list;
        if matches!(
            paint_list.last(),
            Some(ScenePaintCommand::ChildPaintContainer(existing)) if *existing == child_paint_container_id
        ) {
            return;
        }
        paint_list.push(ScenePaintCommand::ChildPaintContainer(child_paint_container_id));
    }

    pub(crate) fn clip(
        &mut self,
        parent_paint_container_id: PaintContainerId,
        parent_clip_id: SceneClipId,
        geometry: SceneClipGeometry,
        _kind: SceneClipKind,
    ) -> SceneClipId {
        let spatial_node_id = self.paint_containers[parent_paint_container_id].spatial_node_id;
        let clip_id = SceneClipId(self.clip_nodes.len());
        self.clip_nodes.push(SceneClipNode {
            parent_clip_id,
            spatial_node_id,
            geometry,
        });
        clip_id
    }

    pub(crate) fn rect_clip(
        &mut self,
        parent_paint_container_id: PaintContainerId,
        parent_clip_id: SceneClipId,
        rect: Rect,
        kind: SceneClipKind,
    ) -> SceneClipId {
        self.clip(
            parent_paint_container_id,
            parent_clip_id,
            SceneClipGeometry::Rect { rect },
            kind,
        )
    }

    pub(crate) fn rounded_rect_clip(
        &mut self,
        parent_paint_container_id: PaintContainerId,
        parent_clip_id: SceneClipId,
        rect: Rect,
        radius: f32,
        kind: SceneClipKind,
    ) -> SceneClipId {
        self.clip(
            parent_paint_container_id,
            parent_clip_id,
            SceneClipGeometry::RoundedRect { rect, radius },
            kind,
        )
    }

    pub(crate) fn deferred_mask_clip(
        &mut self,
        parent_paint_container_id: PaintContainerId,
        parent_clip_id: SceneClipId,
        rect: Rect,
        kind: SceneClipKind,
    ) -> SceneClipId {
        self.clip(
            parent_paint_container_id,
            parent_clip_id,
            SceneClipGeometry::DeferredMask { rect },
            kind,
        )
    }

    pub(crate) fn set_frame_clip(&mut self, paint_container_id: PaintContainerId, clip_id: SceneClipId) {
        self.paint_containers[paint_container_id].clip_id = clip_id;
        let spatial_node_id = self.paint_containers[paint_container_id].spatial_node_id;
        self.spatial_nodes[spatial_node_id.0].clip_chain_root = clip_id;
    }

    pub(crate) fn push_item(
        &mut self,
        paint_container_id: PaintContainerId,
        source: PaintSource<'a>,
        section: StackingContextSection,
        local_origin: DVec2,
        owning_paint_container_id: PaintContainerId,
        clip_id: SceneClipId,
    ) {
        let item_index = self.paint_containers[paint_container_id].items.len();
        self.paint_containers[paint_container_id].items.push(ScenePaintItem {
            source,
            section,
            local_origin,
            owning_paint_container_id,
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
        RenderScene::new(
            self.spatial_nodes,
            self.root_spatial_node_id,
            self.paint_containers,
            self.root_paint_container_id,
            self.clip_nodes,
            RenderPlan::build(owner_semantics),
        )
    }
}
