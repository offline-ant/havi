use makepad_widgets::*;

use crate::paint_items::PaintSource;

pub(crate) type FrameId = usize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum FrameKey {
    Root,
    NodeReferenceFrame(usize),
    NodeStickyFrame(usize),
    NodeScrollFrame(usize),
    NodeIFrameRoot(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FrameKind {
    Root,
    ReferenceFrame,
    StickyFrame,
    ScrollFrame,
    IFrameRoot,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FrameMatrix {
    pub world: Mat4f,
    pub world_inverse: Mat4f,
}

pub(crate) struct FramePaintItem<'a> {
    pub source: PaintSource<'a>,
    pub section: crate::layout_stacking_context::StackingContextSection,
    pub local_origin: DVec2,
    pub clip_id: crate::clip_tree::ClipId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FramePaintCommand {
    Item(usize),
    ChildFrame(FrameId),
}

pub(crate) struct RenderFrame<'a> {
    pub key: FrameKey,
    #[cfg_attr(not(test), allow(dead_code))]
    pub kind: FrameKind,
    pub owner_node_id: Option<usize>,
    pub matrix: FrameMatrix,
    pub clip_id: crate::clip_tree::ClipId,
    pub items: Vec<FramePaintItem<'a>>,
    pub paint_list: Vec<FramePaintCommand>,
}

pub(crate) struct FrameTree<'a> {
    pub frames: Vec<RenderFrame<'a>>,
    pub root: FrameId,
}

impl<'a> FrameTree<'a> {
    pub(crate) fn new() -> Self {
        let mut tree = Self {
            frames: Vec::new(),
            root: 0,
        };
        tree.root = tree.push_root();
        tree
    }

    pub(crate) fn root_id(&self) -> FrameId {
        self.root
    }

    pub(crate) fn frame(&self, id: FrameId) -> &RenderFrame<'a> {
        &self.frames[id]
    }

    pub(crate) fn push_root(&mut self) -> FrameId {
        let identity = Mat4f::identity();
        self.frames.push(RenderFrame {
            key: FrameKey::Root,
            kind: FrameKind::Root,
            owner_node_id: None,
            matrix: FrameMatrix {
                world: identity,
                world_inverse: identity,
            },
            clip_id: crate::clip_tree::ClipId::INVALID,
            items: Vec::new(),
            paint_list: Vec::new(),
        });
        self.frames.len() - 1
    }

    pub(crate) fn push_child_frame(
        &mut self,
        parent: FrameId,
        key: FrameKey,
        kind: FrameKind,
        owner_node_id: Option<usize>,
        local: Mat4f,
    ) -> FrameId {
        let parent_world = self.frames[parent].matrix.world;
        let world = Mat4f::mul(&parent_world, &local);
        let world_inverse = world.invert();
        self.frames.push(RenderFrame {
            key,
            kind,
            owner_node_id,
            matrix: FrameMatrix {
                world,
                world_inverse,
            },
            clip_id: crate::clip_tree::ClipId::INVALID,
            items: Vec::new(),
            paint_list: Vec::new(),
        });
        self.frames.len() - 1
    }

    pub(crate) fn append_child_frame(&mut self, parent: FrameId, child: FrameId) {
        self.frames[parent]
            .paint_list
            .push(FramePaintCommand::ChildFrame(child));
    }

    pub(crate) fn set_clip(&mut self, frame: FrameId, clip_id: crate::clip_tree::ClipId) {
        self.frames[frame].clip_id = clip_id;
    }

    pub(crate) fn push_item(
        &mut self,
        frame: FrameId,
        source: PaintSource<'a>,
        section: crate::layout_stacking_context::StackingContextSection,
        local_origin: DVec2,
        clip_id: crate::clip_tree::ClipId,
    ) {
        let item_index = self.frames[frame].items.len();
        self.frames[frame].items.push(FramePaintItem {
            source,
            section,
            local_origin,
            clip_id,
        });
        self.frames[frame]
            .paint_list
            .push(FramePaintCommand::Item(item_index));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translation(tx: f32, ty: f32) -> Mat4f {
        Mat4f {
            v: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                tx, ty, 0.0, 1.0,
            ],
        }
    }

    #[test]
    fn child_frame_world_matrix_composes_with_parent() {
        let mut tree = FrameTree::new();
        let a = tree.push_child_frame(
            tree.root_id(),
            FrameKey::NodeReferenceFrame(1),
            FrameKind::ReferenceFrame,
            Some(1),
            translation(10.0, 20.0),
        );
        let b = tree.push_child_frame(
            a,
            FrameKey::NodeScrollFrame(2),
            FrameKind::ScrollFrame,
            Some(2),
            translation(-3.0, -4.0),
        );

        assert_eq!(tree.frame(a).matrix.world.v[12], 10.0);
        assert_eq!(tree.frame(a).matrix.world.v[13], 20.0);
        assert_eq!(tree.frame(b).matrix.world.v[12], 7.0);
        assert_eq!(tree.frame(b).matrix.world.v[13], 16.0);
    }

    #[test]
    fn paint_list_preserves_item_and_child_order() {
        let mut tree = FrameTree::new();
        let child = tree.push_child_frame(
            tree.root_id(),
            FrameKey::NodeReferenceFrame(1),
            FrameKind::ReferenceFrame,
            Some(1),
            Mat4f::identity(),
        );
        tree.append_child_frame(tree.root_id(), child);
        assert_eq!(tree.frame(tree.root_id()).paint_list, vec![FramePaintCommand::ChildFrame(child)]);
    }
}
