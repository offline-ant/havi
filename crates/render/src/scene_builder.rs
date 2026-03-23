//! Legacy `RenderScene` construction helpers used only by the fallback path.
//!
//! This path remains only for `HAVI_DISABLE_BROWSER_SCENE=1` and direct-builder
//! fallback failures.

use makepad_compositor::{MpBackfaceVisibility, MpTransformStyle};
use makepad_widgets::{dvec2, Rect};

use crate::scene::{
    RenderClip, RenderClipId, RenderEmbed, RenderEffect, RenderNode, RenderNodeId, RenderPaintItem,
    RenderPaintRun, RenderReferenceFrame, RenderReferenceFrameKind, RenderScene, RenderSceneRoot,
};

pub(crate) struct RenderSceneBuilder<'a> {
    scene: RenderScene<'a>,
}

impl<'a> Default for RenderSceneBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> RenderSceneBuilder<'a> {
    pub(crate) fn new() -> Self {
        let root = RenderNode::ReferenceFrame(RenderReferenceFrame {
            parent: None,
            clip: None,
            local_rect: Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(0.0, 0.0),
            },
            placement_origin: dvec2(0.0, 0.0),
            transform: None,
            perspective: None,
            transform_style: MpTransformStyle::Flat,
            flattens_descendants: false,
            backface_visibility: MpBackfaceVisibility::Visible,
            kind: RenderReferenceFrameKind::Root,
        });
        Self {
            scene: RenderScene::new(RenderSceneRoot::default(), vec![root]),
        }
    }

    pub(crate) fn root_reference_frame_id(&self) -> RenderNodeId {
        self.scene.root_reference_frame_id()
    }

    pub(crate) fn root_reference_frame_mut(&mut self) -> &mut RenderReferenceFrame {
        self.scene.root_reference_frame_mut()
    }

    pub(crate) fn push_reference_frame(&mut self, frame: RenderReferenceFrame) -> RenderNodeId {
        self.push_node(RenderNode::ReferenceFrame(frame))
    }

    pub(crate) fn push_clip(&mut self, clip: RenderClip) -> RenderClipId {
        let node_id = self.push_node(RenderNode::Clip(clip));
        RenderClipId(node_id.0)
    }

    pub(crate) fn push_effect(&mut self, effect: RenderEffect) -> RenderNodeId {
        self.push_node(RenderNode::Effect(effect))
    }

    pub(crate) fn push_paint_run(&mut self, run: RenderPaintRun<'a>) -> RenderNodeId {
        self.push_node(RenderNode::PaintRun(run))
    }

    pub(crate) fn push_item(&mut self, run_id: RenderNodeId, item: RenderPaintItem<'a>) {
        let run = self
            .scene
            .paint_run_mut(run_id)
            .expect("push_item requires a paint run node id");
        run.items.push(item);
    }

    pub(crate) fn push_embed(&mut self, embed: RenderEmbed<'a>) -> RenderNodeId {
        self.push_node(RenderNode::Embed(embed))
    }

    pub(crate) fn build(self) -> RenderScene<'a> {
        self.scene
    }

    fn push_node(&mut self, node: RenderNode<'a>) -> RenderNodeId {
        let node_id = RenderNodeId(self.scene.nodes.len());
        self.scene.nodes.push(node);
        node_id
    }
}
