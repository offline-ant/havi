use crate::clip_tree::{ClipId, ClipTree};
use crate::compositor_scene::CompositorScene;
use crate::frame_tree::{FrameId, FrameKey, FrameKind, FramePaintCommand, FramePaintItem, FrameTree};
use crate::render_plan::RenderPlan;
use crate::scene::RenderScene;
use crate::paint_items::PaintSource;
use crate::layout_stacking_context::StackingContextSection;
use makepad_widgets::*;

pub(crate) struct RenderSceneBuilder<'a> {
    frame_tree: FrameTree<'a>,
    clip_tree: ClipTree,
}

impl<'a> RenderSceneBuilder<'a> {
    pub(crate) fn new() -> Self {
        Self {
            frame_tree: FrameTree::new(),
            clip_tree: ClipTree::new(),
        }
    }

    pub(crate) fn root_frame_id(&self) -> FrameId {
        self.frame_tree.root_id()
    }

    pub(crate) fn child_frame(
        &mut self,
        parent: FrameId,
        key: FrameKey,
        kind: FrameKind,
        owner_node_id: Option<usize>,
        local: Mat4f,
    ) -> FrameId {
        let frame_id = self.frame_tree.push_child_frame(parent, key, kind, owner_node_id, local);
        self.frame_tree.append_child_frame(parent, frame_id);
        frame_id
    }

    pub(crate) fn rect_clip(
        &mut self,
        parent_frame_id: FrameId,
        parent_clip_id: ClipId,
        rect: Rect,
    ) -> ClipId {
        self.clip_tree.push_rect(parent_frame_id, parent_clip_id, rect)
    }

    pub(crate) fn set_frame_clip(&mut self, frame_id: FrameId, clip_id: ClipId) {
        self.frame_tree.set_clip(frame_id, clip_id);
    }

    pub(crate) fn push_item(
        &mut self,
        frame_id: FrameId,
        source: PaintSource<'a>,
        section: StackingContextSection,
        local_origin: DVec2,
        clip_id: ClipId,
    ) {
        self.frame_tree.push_item(frame_id, source, section, local_origin, clip_id);
    }

    pub(crate) fn frame_item(&self, frame_id: FrameId, item_index: usize) -> &FramePaintItem<'a> {
        &self.frame_tree.frame(frame_id).items[item_index]
    }

    pub(crate) fn frame_paint_list(&self, frame_id: FrameId) -> &[FramePaintCommand] {
        &self.frame_tree.frame(frame_id).paint_list
    }

    pub(crate) fn build(
        self,
        owner_semantics: std::collections::HashMap<usize, crate::render_plan::NodeRenderSemantics>,
    ) -> RenderScene<'a> {
        let provisional = RenderScene::from_legacy_parts(
            self.frame_tree,
            self.clip_tree,
            RenderPlan::default(),
            CompositorScene::default(),
        );
        let render_plan = RenderPlan::build(&provisional, owner_semantics);
        let provisional = RenderScene::from_legacy_parts(
            provisional.frame_tree,
            provisional.clip_tree,
            render_plan,
            CompositorScene::default(),
        );
        let compositor_scene = CompositorScene::build(&provisional);
        provisional.with_compositor_scene(compositor_scene)
    }
}
