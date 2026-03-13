use crate::frame_tree::FrameId;
use makepad_widgets::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct ClipId(pub usize);

impl ClipId {
    pub(crate) const INVALID: ClipId = ClipId(usize::MAX);
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ClipNode {
    pub id: ClipId,
    pub parent_clip_id: ClipId,
    pub parent_frame_id: FrameId,
    pub rect: Rect,
}

#[derive(Default)]
pub(crate) struct ClipTree {
    pub nodes: Vec<ClipNode>,
}

impl ClipTree {
    pub(crate) fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub(crate) fn get(&self, id: ClipId) -> &ClipNode {
        &self.nodes[id.0]
    }

    pub(crate) fn push_rect(
        &mut self,
        parent_frame_id: FrameId,
        parent_clip_id: ClipId,
        rect: Rect,
    ) -> ClipId {
        let id = ClipId(self.nodes.len());
        self.nodes.push(ClipNode {
            id,
            parent_clip_id,
            parent_frame_id,
            rect,
        });
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_store_explicit_parentage() {
        let mut clips = ClipTree::new();
        let parent = clips.push_rect(0, ClipId::INVALID, Rect { pos: dvec2(1.0, 2.0), size: dvec2(3.0, 4.0) });
        let child = clips.push_rect(1, parent, Rect { pos: dvec2(5.0, 6.0), size: dvec2(7.0, 8.0) });

        assert_eq!(clips.get(child).parent_clip_id, parent);
        assert_eq!(clips.get(child).parent_frame_id, 1);
        assert_eq!(clips.get(parent).rect.pos, dvec2(1.0, 2.0));
    }
}
