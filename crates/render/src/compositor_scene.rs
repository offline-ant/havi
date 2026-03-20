use std::collections::HashMap;

use crate::makepad_clip::classify_clip_chain;
use crate::render_plan::{CompositorGroupMode, RenderParticipation};
use crate::scene::{PaintContainerId, RenderScene, ScenePaintCommand};

pub(crate) type CompositorSurfaceId = usize;
pub(crate) type CompositorGroupId = usize;

#[derive(Clone, Debug, Default)]
pub(crate) struct CompositorScene {
    pub root_direct_frames: Vec<PaintContainerId>,
    pub root_surfaces: Vec<CompositorSurfaceId>,
    pub surfaces: Vec<CompositorSurface>,
    pub groups: Vec<CompositorGroup>,
    frame_surface: HashMap<PaintContainerId, CompositorSurfaceId>,
}

#[derive(Clone, Debug)]
pub(crate) struct CompositorSurface {
    pub parent_surface_id: Option<CompositorSurfaceId>,
    pub direct_frames: Vec<PaintContainerId>,
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
    pub(crate) fn build(scene: &RenderScene<'_>) -> Self {
        let mut compositor_scene = Self::default();
        compositor_scene.visit_frame(scene, scene.root_paint_container_id(), None, None);
        compositor_scene
    }

    pub(crate) fn frame_redirects_to_surface(
        &self,
        scene: &RenderScene<'_>,
        paint_container_id: PaintContainerId,
        target_paint_container_id: PaintContainerId,
        active_surface_id: Option<CompositorSurfaceId>,
    ) -> bool {
        self.frame_surface(paint_container_id)
            .is_some_and(|surface_id| Some(surface_id) != active_surface_id)
            || self.frame_requires_projected_clip_surface(scene, paint_container_id, target_paint_container_id)
    }

    pub(crate) fn frame_requires_projected_clip_surface(
        &self,
        scene: &RenderScene<'_>,
        paint_container_id: PaintContainerId,
        target_paint_container_id: PaintContainerId,
    ) -> bool {
        if self.frame_surface(paint_container_id).is_some() {
            return false;
        }
        let clip_id = scene.effective_clip_chain_for_paint_container(paint_container_id);
        classify_clip_chain(scene, target_paint_container_id, clip_id)
            .push_result
            .projected_quad_clip_planes
            .is_some()
    }

    pub(crate) fn frame_surface(&self, paint_container_id: PaintContainerId) -> Option<CompositorSurfaceId> {
        self.frame_surface.get(&paint_container_id).copied()
    }

    pub(crate) fn frame_parent_surface(
        &self,
        paint_container_id: PaintContainerId,
    ) -> Option<CompositorSurfaceId> {
        let surface_id = self.frame_surface(paint_container_id)?;
        self.surfaces[surface_id].parent_surface_id
    }

    fn visit_frame(
        &mut self,
        scene: &RenderScene<'_>,
        paint_container_id: PaintContainerId,
        current_surface_id: Option<CompositorSurfaceId>,
        current_preserve_group_id: Option<CompositorGroupId>,
    ) {
        let target_paint_container_id = current_surface_id
            .and_then(|surface_id| self.surfaces.get(surface_id))
            .and_then(|surface| surface.direct_frames.first().copied())
            .unwrap_or(scene.root_paint_container_id());
        let requires_projected_clip_surface = self.frame_requires_projected_clip_surface(
            scene,
            paint_container_id,
            target_paint_container_id,
        );
        match scene.frame_participation(paint_container_id) {
            RenderParticipation::Direct2d if !requires_projected_clip_surface => {
                self.push_direct_frame(paint_container_id, current_surface_id);
                for child_paint_container_id in child_frame_ids(scene, paint_container_id) {
                    self.visit_frame(
                        scene,
                        child_paint_container_id,
                        current_surface_id,
                        current_preserve_group_id,
                    );
                }
            }
            RenderParticipation::Direct2d => {
                let surface_id = self.push_surface(paint_container_id, current_surface_id, CompositorGroupMode::Flat);
                self.push_direct_frame(paint_container_id, Some(surface_id));
                for child_paint_container_id in child_frame_ids(scene, paint_container_id) {
                    self.visit_frame(
                        scene,
                        child_paint_container_id,
                        Some(surface_id),
                        None,
                    );
                }
            }
            RenderParticipation::Compositor { group } => {
                let spatial_node = scene.spatial_node(scene.paint_container_spatial_node_id(paint_container_id));
                let group = match spatial_node.semantics {
                    crate::scene::SpatialNodeSemantics::ReferenceFrame(data)
                        if data.has_perspective || data.preserves_3d => CompositorGroupMode::Preserve3d,
                    _ => group,
                };
                let surface_id = self.push_surface(paint_container_id, current_surface_id, group);
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

                self.push_direct_frame(paint_container_id, Some(surface_id));
                for child_paint_container_id in child_frame_ids(scene, paint_container_id) {
                    self.visit_frame(
                        scene,
                        child_paint_container_id,
                        Some(surface_id),
                        next_preserve_group_id,
                    );
                }
            }
        }
    }

    fn push_direct_frame(
        &mut self,
        paint_container_id: PaintContainerId,
        current_surface_id: Option<CompositorSurfaceId>,
    ) {
        if let Some(surface_id) = current_surface_id {
            self.surfaces[surface_id].direct_frames.push(paint_container_id);
        } else {
            self.root_direct_frames.push(paint_container_id);
        }
    }

    fn push_surface(
        &mut self,
        paint_container_id: PaintContainerId,
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
        self.frame_surface.insert(paint_container_id, surface_id);
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

fn child_frame_ids(scene: &RenderScene<'_>, paint_container_id: PaintContainerId) -> Vec<PaintContainerId> {
    scene
        .frame_paint_list(paint_container_id)
        .iter()
        .filter_map(|command| match command {
            ScenePaintCommand::ChildPaintContainer(child_paint_container_id) => Some(*child_paint_container_id),
            ScenePaintCommand::Item(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_builder::build_scene;
    use crate::render_plan::RenderPlan;

    #[test]
    fn empty_scene_has_no_surfaces() {
        let scene = build_scene(&[], &crate::ScrollState::default(), dvec2(0.0, 0.0), dvec2(100.0, 100.0));
        let compositor = CompositorScene::build(&scene);
        assert!(compositor.root_surfaces.is_empty());
        assert!(compositor.root_direct_frames.contains(&scene.root_paint_container_id()));
        let _ = RenderPlan::default();
    }
}
