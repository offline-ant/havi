use havi_types::Fragment;
use makepad_widgets::*;

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
    pub local: Mat4f,
    pub world: Mat4f,
    pub world_inverse: Mat4f,
}

pub(crate) struct FramePaintItem<'a> {
    pub fragment: &'a Fragment,
    pub section: crate::stacking_context::StackingContextSection,
    pub local_origin: DVec2,
    pub clip_id: crate::clip_tree::ClipId,
}

pub(crate) struct RenderFrame<'a> {
    pub id: FrameId,
    pub key: FrameKey,
    pub kind: FrameKind,
    pub owner_node_id: Option<usize>,
    pub parent: Option<FrameId>,
    pub children: Vec<FrameId>,
    pub matrix: FrameMatrix,
    pub items: Vec<FramePaintItem<'a>>,
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

    pub(crate) fn frame_mut(&mut self, id: FrameId) -> &mut RenderFrame<'a> {
        &mut self.frames[id]
    }

    pub(crate) fn push_root(&mut self) -> FrameId {
        let id = self.frames.len();
        let identity = Mat4f::identity();
        self.frames.push(RenderFrame {
            id,
            key: FrameKey::Root,
            kind: FrameKind::Root,
            owner_node_id: None,
            parent: None,
            children: Vec::new(),
            matrix: FrameMatrix {
                local: identity,
                world: identity,
                world_inverse: identity,
            },
            items: Vec::new(),
        });
        id
    }

    pub(crate) fn push_child_frame(
        &mut self,
        parent: FrameId,
        key: FrameKey,
        kind: FrameKind,
        owner_node_id: Option<usize>,
        local: Mat4f,
    ) -> FrameId {
        let id = self.frames.len();
        let parent_world = self.frames[parent].matrix.world;
        let world = Mat4f::mul(&parent_world, &local);
        let world_inverse = world.invert();
        self.frames.push(RenderFrame {
            id,
            key,
            kind,
            owner_node_id,
            parent: Some(parent),
            children: Vec::new(),
            matrix: FrameMatrix {
                local,
                world,
                world_inverse,
            },
            items: Vec::new(),
        });
        self.frames[parent].children.push(id);
        id
    }

    pub(crate) fn push_item(
        &mut self,
        frame: FrameId,
        fragment: &'a Fragment,
        section: crate::stacking_context::StackingContextSection,
        local_origin: DVec2,
        clip_id: crate::clip_tree::ClipId,
    ) {
        self.frames[frame].items.push(FramePaintItem {
            fragment,
            section,
            local_origin,
            clip_id,
        });
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
}
