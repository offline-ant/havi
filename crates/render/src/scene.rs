//! Legacy intermediate scene types used only by the fallback render path.
//!
//! This path remains only for `HAVI_DISABLE_BROWSER_SCENE=1` and direct-builder
//! fallback failures.

use makepad_widgets::*;

use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::PaintSource;
use makepad_compositor::{MpBackfaceVisibility, MpTransformStyle};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct RenderNodeId(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct RenderClipId(pub usize);

#[derive(Clone, Debug, Default)]
pub(crate) struct RenderSceneRoot {
    pub clip: Option<RenderClipId>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RenderReferenceFrame {
    pub parent: Option<RenderNodeId>,
    pub clip: Option<RenderClipId>,
    pub local_rect: Rect,
    pub placement_origin: DVec2,
    pub transform: Option<Mat4f>,
    pub perspective: Option<Mat4f>,
    pub transform_style: MpTransformStyle,
    pub flattens_descendants: bool,
    pub backface_visibility: MpBackfaceVisibility,
    pub kind: RenderReferenceFrameKind,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum RenderReferenceFrameKind {
    Root,
    Transform,
    Scroll(RenderScrollInfo),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RenderScrollInfo {
    pub scroll_offset: DVec2,
}

#[derive(Clone, Debug)]
pub(crate) struct RenderClip {
    pub parent: Option<RenderNodeId>,
    pub prev: Option<RenderClipId>,
    pub geometry: RenderClipGeometry,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RenderClipGeometry {
    Rect { rect: Rect },
    RoundedRect { rect: Rect, radius: f32 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RenderFilterSet {
    pub entries: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum RenderBlendMode {
    #[default]
    Normal,
    Named(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RenderMask {
    Rect { rect: Rect },
}

#[derive(Clone, Debug)]
pub(crate) struct RenderEffect {
    pub parent: RenderNodeId,
    pub clip: Option<RenderClipId>,
    pub opacity: f32,
    pub filter: RenderFilterSet,
    pub blend_mode: RenderBlendMode,
    pub is_isolated: bool,
    pub mask: Option<RenderMask>,
}

#[derive(Clone)]
pub(crate) struct RenderPaintRun<'a> {
    pub parent: RenderNodeId,
    pub clip: Option<RenderClipId>,
    pub local_bounds: Rect,
    pub items: Vec<RenderPaintItem<'a>>,
}

#[derive(Clone, Copy)]
pub(crate) struct RenderPaintItem<'a> {
    pub section: StackingContextSection,
    pub local_origin: DVec2,
    pub source: PaintSource<'a>,
}

#[derive(Clone)]
pub(crate) struct RenderEmbed<'a> {
    pub parent: RenderNodeId,
    pub clip: Option<RenderClipId>,
    pub local_rect: Rect,
    pub child_scene: Box<RenderScene<'a>>,
}

#[derive(Clone)]
pub(crate) enum RenderNode<'a> {
    ReferenceFrame(RenderReferenceFrame),
    Clip(RenderClip),
    Effect(RenderEffect),
    PaintRun(RenderPaintRun<'a>),
    Embed(RenderEmbed<'a>),
}

#[derive(Clone)]
pub(crate) struct RenderScene<'a> {
    pub root: RenderSceneRoot,
    pub nodes: Vec<RenderNode<'a>>,
}

impl<'a> RenderScene<'a> {
    pub(crate) fn new(root: RenderSceneRoot, nodes: Vec<RenderNode<'a>>) -> Self {
        debug_assert!(matches!(
            nodes.first(),
            Some(RenderNode::ReferenceFrame(RenderReferenceFrame {
                kind: RenderReferenceFrameKind::Root,
                ..
            }))
        ));
        Self { root, nodes }
    }

    pub(crate) fn root_reference_frame_id(&self) -> RenderNodeId {
        RenderNodeId(0)
    }

    pub(crate) fn root_reference_frame(&self) -> &RenderReferenceFrame {
        self.reference_frame(self.root_reference_frame_id())
            .expect("root node must be a root reference frame")
    }

    pub(crate) fn root_reference_frame_mut(&mut self) -> &mut RenderReferenceFrame {
        let root = self.root_reference_frame_id();
        self.reference_frame_mut(root)
            .expect("root node must be a root reference frame")
    }

    pub(crate) fn node(&self, id: RenderNodeId) -> &RenderNode<'a> {
        &self.nodes[id.0]
    }

    pub(crate) fn node_mut(&mut self, id: RenderNodeId) -> &mut RenderNode<'a> {
        &mut self.nodes[id.0]
    }

    pub(crate) fn reference_frame(&self, id: RenderNodeId) -> Option<&RenderReferenceFrame> {
        match self.node(id) {
            RenderNode::ReferenceFrame(frame) => Some(frame),
            _ => None,
        }
    }

    pub(crate) fn reference_frame_mut(
        &mut self,
        id: RenderNodeId,
    ) -> Option<&mut RenderReferenceFrame> {
        match self.node_mut(id) {
            RenderNode::ReferenceFrame(frame) => Some(frame),
            _ => None,
        }
    }

    pub(crate) fn paint_run_mut(&mut self, id: RenderNodeId) -> Option<&mut RenderPaintRun<'a>> {
        match self.node_mut(id) {
            RenderNode::PaintRun(run) => Some(run),
            _ => None,
        }
    }
}
