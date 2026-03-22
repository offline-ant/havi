use makepad_widgets::*;

use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::PaintSource;
use makepad_compositor::{MpBackfaceVisibility, MpTransformStyle};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct RenderNodeId(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct RenderClipId(pub usize);

impl RenderClipId {
    pub(crate) const INVALID: Self = Self(usize::MAX);

    pub(crate) fn is_valid(self) -> bool {
        self != Self::INVALID
    }
}

pub(crate) type PaintContainerId = usize;
pub(crate) type SpatialNodeId = RenderNodeId;
pub(crate) type SceneClipId = RenderClipId;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReferenceFrameData {
    pub placement_origin: DVec2,
    pub transform_matrix: Option<Mat4f>,
    pub perspective_matrix: Option<Mat4f>,
    pub transform_style: MpTransformStyle,
    pub flattens_descendants: bool,
    pub backface_visibility: MpBackfaceVisibility,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StickyOffsetConstraints {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StickyOffsetBounds {
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StickyNodeData {
    pub frame_rect: Rect,
    pub margins: StickyOffsetConstraints,
    pub vertical_offset_bounds: StickyOffsetBounds,
    pub horizontal_offset_bounds: StickyOffsetBounds,
    pub containing_block_rect: Rect,
    pub scroll_frame_rect: Rect,
    pub scroll_port_rect: Rect,
    pub nearest_scroll_node_id: Option<SpatialNodeId>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ScrollNodeData {
    pub scroll_offset: DVec2,
    pub scroll_frame_rect: Rect,
    pub sensitivity_x: bool,
    pub sensitivity_y: bool,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneClipKind {
    Overflow,
    OverflowClip,
    CssClip,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PaintContainerKind {
    Root,
    Normal,
    IFrameRoot { size: DVec2 },
}

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
    Sticky(RenderStickyInfo),
    IFrameRoot { size: DVec2 },
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RenderScrollInfo {
    pub scroll_offset: DVec2,
    pub scroll_frame_rect: Rect,
    pub sensitivity_x: bool,
    pub sensitivity_y: bool,
    pub external_scroll_node_id: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RenderStickyOffsetConstraints {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RenderStickyOffsetBounds {
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RenderStickyInfo {
    pub frame_rect: Rect,
    pub margins: RenderStickyOffsetConstraints,
    pub vertical_offset_bounds: RenderStickyOffsetBounds,
    pub horizontal_offset_bounds: RenderStickyOffsetBounds,
    pub containing_block_rect: Rect,
    pub scroll_frame_rect: Rect,
    pub scroll_port_rect: Rect,
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
    PlaneSet { planes: Vec<Vec4f> },
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
    RoundedRect { rect: Rect, radius: f32 },
    PlaneSet { planes: Vec<Vec4f> },
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
    pub owner_node_id: Option<usize>,
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
    pub owner_node_id: Option<usize>,
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

    pub(crate) fn root_paint_container_id(&self) -> PaintContainerId {
        0
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

    pub(crate) fn clip(&self, id: RenderClipId) -> Option<&RenderClip> {
        if !id.is_valid() {
            return None;
        }
        match self.nodes.get(id.0) {
            Some(RenderNode::Clip(clip)) => Some(clip),
            _ => None,
        }
    }

    pub(crate) fn clip_mut(&mut self, id: RenderClipId) -> Option<&mut RenderClip> {
        if !id.is_valid() {
            return None;
        }
        match self.nodes.get_mut(id.0) {
            Some(RenderNode::Clip(clip)) => Some(clip),
            _ => None,
        }
    }

    pub(crate) fn effect(&self, id: RenderNodeId) -> Option<&RenderEffect> {
        match self.node(id) {
            RenderNode::Effect(effect) => Some(effect),
            _ => None,
        }
    }

    pub(crate) fn effect_mut(&mut self, id: RenderNodeId) -> Option<&mut RenderEffect> {
        match self.node_mut(id) {
            RenderNode::Effect(effect) => Some(effect),
            _ => None,
        }
    }

    pub(crate) fn paint_run(&self, id: RenderNodeId) -> Option<&RenderPaintRun<'a>> {
        match self.node(id) {
            RenderNode::PaintRun(run) => Some(run),
            _ => None,
        }
    }

    pub(crate) fn paint_run_mut(&mut self, id: RenderNodeId) -> Option<&mut RenderPaintRun<'a>> {
        match self.node_mut(id) {
            RenderNode::PaintRun(run) => Some(run),
            _ => None,
        }
    }

    pub(crate) fn embed(&self, id: RenderNodeId) -> Option<&RenderEmbed<'a>> {
        match self.node(id) {
            RenderNode::Embed(embed) => Some(embed),
            _ => None,
        }
    }

    pub(crate) fn embed_mut(&mut self, id: RenderNodeId) -> Option<&mut RenderEmbed<'a>> {
        match self.node_mut(id) {
            RenderNode::Embed(embed) => Some(embed),
            _ => None,
        }
    }

    pub(crate) fn parent(&self, id: RenderNodeId) -> Option<RenderNodeId> {
        match self.node(id) {
            RenderNode::ReferenceFrame(frame) => frame.parent,
            RenderNode::Clip(clip) => clip.parent,
            RenderNode::Effect(effect) => Some(effect.parent),
            RenderNode::PaintRun(run) => Some(run.parent),
            RenderNode::Embed(embed) => Some(embed.parent),
        }
    }

    pub(crate) fn children_of(
        &self,
        parent: RenderNodeId,
    ) -> impl Iterator<Item = (RenderNodeId, &RenderNode<'a>)> + '_ {
        self.nodes.iter().enumerate().filter_map(move |(index, node)| {
            let node_id = RenderNodeId(index);
            (node_id != parent && self.parent(node_id) == Some(parent)).then_some((node_id, node))
        })
    }

    pub(crate) fn root_children(&self) -> impl Iterator<Item = (RenderNodeId, &RenderNode<'a>)> + '_ {
        self.children_of(self.root_reference_frame_id())
    }
}
