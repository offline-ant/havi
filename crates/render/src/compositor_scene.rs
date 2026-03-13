use std::collections::HashMap;

use crate::frame_tree::{FrameId, FramePaintCommand, FrameTree};
use crate::render_plan::{CompositorGroupMode, RenderParticipation, RenderPlan};

pub(crate) type CompositorSurfaceId = usize;
pub(crate) type CompositorGroupId = usize;

#[derive(Clone, Debug, Default)]
pub(crate) struct CompositorScene {
    pub root_direct_frames: Vec<FrameId>,
    pub root_surfaces: Vec<CompositorSurfaceId>,
    pub surfaces: Vec<CompositorSurface>,
    pub groups: Vec<CompositorGroup>,
    frame_surface: HashMap<FrameId, CompositorSurfaceId>,
}

#[derive(Clone, Debug)]
pub(crate) struct CompositorSurface {
    pub parent_surface_id: Option<CompositorSurfaceId>,
    pub direct_frames: Vec<FrameId>,
    pub child_surfaces: Vec<CompositorSurfaceId>,
    pub started_group_id: Option<CompositorGroupId>,
    pub participates_in_group_id: Option<CompositorGroupId>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub flattening_boundary: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CompositorGroup {
    #[cfg_attr(not(test), allow(dead_code))]
    pub mode: CompositorGroupMode,
    pub member_surfaces: Vec<CompositorSurfaceId>,
}

impl CompositorScene {
    pub(crate) fn build(frame_tree: &FrameTree<'_>, render_plan: &RenderPlan) -> Self {
        let mut scene = Self::default();
        scene.visit_frame(frame_tree, render_plan, frame_tree.root_id(), None, None);
        scene
    }

    pub(crate) fn frame_surface(&self, frame_id: FrameId) -> Option<CompositorSurfaceId> {
        self.frame_surface.get(&frame_id).copied()
    }

    pub(crate) fn frame_parent_surface(
        &self,
        frame_id: FrameId,
    ) -> Option<CompositorSurfaceId> {
        let surface_id = self.frame_surface(frame_id)?;
        self.surfaces[surface_id].parent_surface_id
    }

    fn visit_frame(
        &mut self,
        frame_tree: &FrameTree<'_>,
        render_plan: &RenderPlan,
        frame_id: FrameId,
        current_surface_id: Option<CompositorSurfaceId>,
        current_preserve_group_id: Option<CompositorGroupId>,
    ) {
        match render_plan.frame_participation(frame_id) {
            RenderParticipation::Direct2d => {
                self.push_direct_frame(frame_id, current_surface_id);
                for child_frame_id in child_frame_ids(frame_tree, frame_id) {
                    self.visit_frame(
                        frame_tree,
                        render_plan,
                        child_frame_id,
                        current_surface_id,
                        current_preserve_group_id,
                    );
                }
            }
            RenderParticipation::Compositor { group } => {
                let surface_id = self.push_surface(frame_id, current_surface_id, group);
                if let Some(group_id) = current_preserve_group_id {
                    self.surfaces[surface_id].participates_in_group_id = Some(group_id);
                    self.groups[group_id].member_surfaces.push(surface_id);
                }

                let next_preserve_group_id = match group {
                    CompositorGroupMode::Flat => None,
                    CompositorGroupMode::Preserve3d => match current_preserve_group_id {
                        Some(group_id) => Some(group_id),
                        None => {
                            let group_id = self.push_group(CompositorGroupMode::Preserve3d, surface_id);
                            self.surfaces[surface_id].started_group_id = Some(group_id);
                            self.surfaces[surface_id].participates_in_group_id = Some(group_id);
                            Some(group_id)
                        }
                    },
                };

                self.push_direct_frame(frame_id, Some(surface_id));
                for child_frame_id in child_frame_ids(frame_tree, frame_id) {
                    self.visit_frame(
                        frame_tree,
                        render_plan,
                        child_frame_id,
                        Some(surface_id),
                        next_preserve_group_id,
                    );
                }
            }
        }
    }

    fn push_direct_frame(
        &mut self,
        frame_id: FrameId,
        current_surface_id: Option<CompositorSurfaceId>,
    ) {
        if let Some(surface_id) = current_surface_id {
            self.surfaces[surface_id].direct_frames.push(frame_id);
        } else {
            self.root_direct_frames.push(frame_id);
        }
    }

    fn push_surface(
        &mut self,
        frame_id: FrameId,
        parent_surface_id: Option<CompositorSurfaceId>,
        group_mode: CompositorGroupMode,
    ) -> CompositorSurfaceId {
        let flattening_boundary = matches!(group_mode, CompositorGroupMode::Flat);
        let surface_id = self.surfaces.len();
        self.surfaces.push(CompositorSurface {
            parent_surface_id,
            direct_frames: Vec::new(),
            child_surfaces: Vec::new(),
            started_group_id: None,
            participates_in_group_id: None,
            flattening_boundary,
        });
        self.frame_surface.insert(frame_id, surface_id);
        if let Some(parent_surface_id) = parent_surface_id {
            self.surfaces[parent_surface_id].child_surfaces.push(surface_id);
        } else {
            self.root_surfaces.push(surface_id);
        }
        if flattening_boundary {
            let group_id = self.push_group(CompositorGroupMode::Flat, surface_id);
            self.surfaces[surface_id].started_group_id = Some(group_id);
            self.surfaces[surface_id].participates_in_group_id = Some(group_id);
        }
        surface_id
    }

    fn push_group(
        &mut self,
        mode: CompositorGroupMode,
        owner_surface_id: CompositorSurfaceId,
    ) -> CompositorGroupId {
        let group_id = self.groups.len();
        self.groups.push(CompositorGroup {
            mode,
            member_surfaces: vec![owner_surface_id],
        });
        group_id
    }
}

fn child_frame_ids(frame_tree: &FrameTree<'_>, frame_id: FrameId) -> Vec<FrameId> {
    frame_tree
        .frame(frame_id)
        .paint_list
        .iter()
        .filter_map(|command| match command {
            FramePaintCommand::ChildFrame(child_frame_id) => Some(*child_frame_id),
            FramePaintCommand::Item(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_tree::{FrameKind, FrameKey};
    use makepad_widgets::Mat4f;

    fn build_plan_with_frames(
        frame_participation: Vec<RenderParticipation>,
        edges: &[(FrameId, FrameId)],
    ) -> (FrameTree<'static>, RenderPlan) {
        let mut tree = FrameTree::new();
        let root = tree.root_id();
        while tree.frames.len() < frame_participation.len() {
            let id = tree.frames.len();
            tree.push_child_frame(
                root,
                FrameKey::NodeReferenceFrame(id),
                FrameKind::ReferenceFrame,
                Some(id),
                Mat4f::identity(),
            );
        }
        tree.frames[root].paint_list.clear();
        for (parent, child) in edges {
            tree.append_child_frame(*parent, *child);
        }
        let plan = RenderPlan::from_frame_participation(frame_participation);
        (tree, plan)
    }

    #[test]
    fn preserve3d_descendants_share_ancestor_group_until_flat_boundary() {
        let (tree, plan) = build_plan_with_frames(
            vec![
                RenderParticipation::Direct2d,
                RenderParticipation::Compositor {
                    group: CompositorGroupMode::Preserve3d,
                },
                RenderParticipation::Compositor {
                    group: CompositorGroupMode::Preserve3d,
                },
            ],
            &[(0, 1), (1, 2)],
        );
        let scene = CompositorScene::build(&tree, &plan);

        let ancestor_surface = scene.frame_surface(1).unwrap();
        let child_surface = scene.frame_surface(2).unwrap();
        let group_id = scene.surfaces[ancestor_surface].started_group_id.unwrap();

        assert_eq!(scene.groups[group_id].mode, CompositorGroupMode::Preserve3d);
        assert_eq!(scene.surfaces[child_surface].started_group_id, None);
        assert_eq!(scene.surfaces[child_surface].participates_in_group_id, Some(group_id));
        assert_eq!(scene.groups[group_id].member_surfaces, vec![ancestor_surface, child_surface]);
    }

    #[test]
    fn flat_surface_creates_flattening_boundary_and_new_flat_group() {
        let (tree, plan) = build_plan_with_frames(
            vec![
                RenderParticipation::Direct2d,
                RenderParticipation::Compositor {
                    group: CompositorGroupMode::Flat,
                },
            ],
            &[(0, 1)],
        );
        let scene = CompositorScene::build(&tree, &plan);
        let surface_id = scene.frame_surface(1).unwrap();
        let group_id = scene.surfaces[surface_id].started_group_id.unwrap();

        assert!(scene.surfaces[surface_id].flattening_boundary);
        assert_eq!(scene.groups[group_id].mode, CompositorGroupMode::Flat);
        assert_eq!(scene.groups[group_id].member_surfaces, vec![surface_id]);
    }

    #[test]
    fn flat_descendant_breaks_preserve3d_participation_for_grandchildren() {
        let (tree, plan) = build_plan_with_frames(
            vec![
                RenderParticipation::Direct2d,
                RenderParticipation::Compositor {
                    group: CompositorGroupMode::Preserve3d,
                },
                RenderParticipation::Compositor {
                    group: CompositorGroupMode::Flat,
                },
                RenderParticipation::Compositor {
                    group: CompositorGroupMode::Preserve3d,
                },
            ],
            &[(0, 1), (1, 2), (2, 3)],
        );
        let scene = CompositorScene::build(&tree, &plan);

        let root_preserve_surface = scene.frame_surface(1).unwrap();
        let flat_surface = scene.frame_surface(2).unwrap();
        let nested_preserve_surface = scene.frame_surface(3).unwrap();
        let root_group = scene.surfaces[root_preserve_surface].started_group_id.unwrap();
        let nested_group = scene.surfaces[nested_preserve_surface].started_group_id.unwrap();

        assert_eq!(scene.surfaces[flat_surface].participates_in_group_id, Some(root_group));
        assert_eq!(scene.surfaces[nested_preserve_surface].participates_in_group_id, Some(nested_group));
        assert_ne!(root_group, nested_group);
        assert_eq!(scene.groups[root_group].member_surfaces, vec![root_preserve_surface, flat_surface]);
        assert_eq!(scene.groups[nested_group].member_surfaces, vec![nested_preserve_surface]);
    }
}
