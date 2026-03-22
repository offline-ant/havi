use std::collections::HashMap;

use makepad_compositor::{
    MpBackfaceVisibility, MpBlendMode, MpClipNode, MpClipShape, MpEffectNode, MpEmbedNode,
    MpFilterSet, MpMaskSource, MpNode, MpNodeId, MpReferenceFrame, MpRenderer, MpScene,
    MpSceneRoot, MpSurface, MpSurfaceColorFormat, MpSurfaceNode, MpSurfaceSource,
    MpTransformStyle,
};
use makepad_widgets::makepad_draw::draw_list_2d::DrawList2d;
use makepad_widgets::*;

use crate::makepad_builder::MakepadDrawState;
use crate::makepad_fragments::paint_fragment_item;
use crate::scene::{
    PaintContainerId, PaintContainerKind, RenderScene, SceneClipGeometry, SceneClipId,
    ScenePaintCommand, ScenePaintItem, SpatialNodeSemantics,
};
use crate::{BackendRootBasis, SceneSurfaceCacheEntry, SceneSurfaceKey};

pub(crate) fn draw_render_scene(
    cx: &mut Cx2d,
    render_scene: &RenderScene<'_>,
    backend_root_basis: BackendRootBasis,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
) {
    if state.frame_draw_lists.renderer.is_none() {
        state.frame_draw_lists.renderer = Some(MpRenderer::new(cx.cx));
    }

    let host_rect = Rect {
        pos: backend_root_basis.webview_origin,
        size: root_viewport_size,
    };
    let page_to_host = backend_root_basis.page_to_pass_transform();
    let mut lowering = MpSceneLowering::new(
        render_scene,
        render_scene.root_paint_container_id(),
        host_rect,
        page_to_host,
        true,
    );
    let scene = lowering.lower_scene(cx, state);
    if let Err(err) = state
        .frame_draw_lists
        .renderer
        .as_mut()
        .unwrap()
        .draw_scene(cx, &scene)
    {
        eprintln!("[havi][render] draw_scene error: {err:?}");
    }
}

struct MpSceneLowering<'a> {
    render_scene: &'a RenderScene<'a>,
    root_paint_container_id: PaintContainerId,
    host_rect: Rect,
    page_to_host: Mat4f,
    use_root_clip: bool,
    clip_map: HashMap<SceneClipId, MpNodeId>,
    bounds_cache: HashMap<PaintContainerId, Option<Rect>>,
}

impl<'a> MpSceneLowering<'a> {
    fn new(
        render_scene: &'a RenderScene<'a>,
        root_paint_container_id: PaintContainerId,
        host_rect: Rect,
        page_to_host: Mat4f,
        use_root_clip: bool,
    ) -> Self {
        Self {
            render_scene,
            root_paint_container_id,
            host_rect,
            page_to_host,
            use_root_clip,
            clip_map: HashMap::new(),
            bounds_cache: HashMap::new(),
        }
    }

    fn lower_scene(&mut self, cx: &mut Cx2d, state: &mut MakepadDrawState<'_>) -> MpScene {
        let mut scene = MpScene::new(MpSceneRoot {
            host_rect: self.host_rect,
            page_to_host: self.page_to_host,
            clip: None,
        });
        let root_ref = scene.push(MpNode::ReferenceFrame(MpReferenceFrame {
            parent: None,
            clip: None,
            local_rect: self
                .paint_container_local_bounds(self.root_paint_container_id)
                .unwrap_or(Rect {
                    pos: dvec2(0.0, 0.0),
                    size: self.host_rect.size,
                }),
            transform: Mat4f::identity(),
            perspective: None,
            transform_style: MpTransformStyle::Flat,
            backface_visibility: MpBackfaceVisibility::Visible,
            flattens_descendants: true,
        }));
        if self.use_root_clip {
            scene.root.clip = self.lower_clip_chain(
                &mut scene,
                self.render_scene
                    .effective_clip_chain_for_paint_container(self.root_paint_container_id),
                root_ref,
            );
        }
        self.lower_paint_container_contents(
            cx,
            state,
            &mut scene,
            self.root_paint_container_id,
            root_ref,
            self.root_paint_container_id,
        );
        scene
    }

    fn lower_paint_container_boundary(
        &mut self,
        cx: &mut Cx2d,
        state: &mut MakepadDrawState<'_>,
        scene: &mut MpScene,
        parent_paint_container_id: PaintContainerId,
        parent: MpNodeId,
        paint_container_id: PaintContainerId,
        use_container_clip: bool,
    ) {
        let reference_frame = scene.push(MpNode::ReferenceFrame(
            self.reference_frame_for_paint_container(
                parent_paint_container_id,
                parent,
                paint_container_id,
            ),
        ));
        let clip = if use_container_clip {
            self.lower_clip_chain(
                scene,
                self.render_scene
                    .effective_clip_chain_for_paint_container(paint_container_id),
                reference_frame,
            )
        } else {
            None
        };
        if let Some(MpNode::ReferenceFrame(frame)) = scene.nodes.get_mut(reference_frame) {
            frame.clip = clip;
        }

        let parent_for_contents = match self.effect_semantics_for_boundary(
            parent_paint_container_id,
            paint_container_id,
        ) {
            Some(semantics) => scene.push(MpNode::Effect(MpEffectNode {
                parent: reference_frame,
                clip: None,
                opacity: semantics.opacity,
                filter: MpFilterSet { entries: Vec::new() },
                blend_mode: if semantics.needs_blend {
                    MpBlendMode::Named("mix-blend-mode".to_string())
                } else {
                    MpBlendMode::Normal
                },
                is_isolated: semantics.needs_isolation,
                mask: if semantics.needs_mask {
                    clip.map(MpMaskSource::Clip)
                } else {
                    None
                },
            })),
            None => reference_frame,
        };

        match self.render_scene.paint_container_kind(paint_container_id) {
            PaintContainerKind::IFrameRoot { size } => {
                let child_scene = self.lower_embedded_scene(cx, state, paint_container_id, size);
                scene.push(MpNode::Embed(MpEmbedNode {
                    parent: parent_for_contents,
                    clip: None,
                    local_rect: Rect {
                        pos: dvec2(0.0, 0.0),
                        size,
                    },
                    child_scene: Box::new(child_scene),
                }));
            }
            PaintContainerKind::Root | PaintContainerKind::Normal => {
                self.lower_paint_container_contents(
                    cx,
                    state,
                    scene,
                    paint_container_id,
                    parent_for_contents,
                    paint_container_id,
                );
            }
        }
    }

    fn lower_paint_container_contents(
        &mut self,
        cx: &mut Cx2d,
        state: &mut MakepadDrawState<'_>,
        scene: &mut MpScene,
        current_paint_container_id: PaintContainerId,
        parent: MpNodeId,
        content_paint_container_id: PaintContainerId,
    ) {
        let mut run = Vec::new();
        let mut run_index = 0;
        for command in self
            .render_scene
            .frame_paint_list(content_paint_container_id)
            .iter()
            .copied()
        {
            match command {
                ScenePaintCommand::Item(item_index) => run.push(item_index),
                ScenePaintCommand::ChildPaintContainer(child_paint_container_id) => {
                    if !run.is_empty() {
                        self.lower_item_run(
                            cx,
                            state,
                            scene,
                            parent,
                            content_paint_container_id,
                            run_index,
                            &run,
                        );
                        run.clear();
                        run_index += 1;
                    }
                    self.lower_paint_container_boundary(
                        cx,
                        state,
                        scene,
                        current_paint_container_id,
                        parent,
                        child_paint_container_id,
                        true,
                    );
                }
            }
        }
        if !run.is_empty() {
            self.lower_item_run(
                cx,
                state,
                scene,
                parent,
                content_paint_container_id,
                run_index,
                &run,
            );
        }
    }

    fn lower_embedded_scene(
        &mut self,
        cx: &mut Cx2d,
        state: &mut MakepadDrawState<'_>,
        paint_container_id: PaintContainerId,
        size: DVec2,
    ) -> MpScene {
        let mut lowering = MpSceneLowering::new(
            self.render_scene,
            paint_container_id,
            Rect {
                pos: dvec2(0.0, 0.0),
                size,
            },
            Mat4f::identity(),
            true,
        );
        lowering.lower_scene(cx, state)
    }

    fn reference_frame_for_paint_container(
        &mut self,
        parent_paint_container_id: PaintContainerId,
        parent: MpNodeId,
        paint_container_id: PaintContainerId,
    ) -> MpReferenceFrame {
        let bounds = self
            .paint_container_local_bounds(paint_container_id)
            .or_else(|| match self.render_scene.paint_container_kind(paint_container_id) {
                PaintContainerKind::IFrameRoot { size } => Some(Rect {
                    pos: dvec2(0.0, 0.0),
                    size,
                }),
                _ => None,
            })
            .unwrap_or(Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(1.0, 1.0),
            });
        let spatial_node_id = self.render_scene.paint_container_spatial_node_id(paint_container_id);
        let parent_spatial_node_id = self
            .render_scene
            .paint_container_spatial_node_id(parent_paint_container_id);
        let (transform, perspective, transform_style, backface_visibility, flattens_descendants) =
            if spatial_node_id == parent_spatial_node_id {
                (
                    Mat4f::identity(),
                    None,
                    MpTransformStyle::Flat,
                    MpBackfaceVisibility::Visible,
                    true,
                )
            } else {
                match self.render_scene.spatial_node(spatial_node_id).semantics {
                    SpatialNodeSemantics::ReferenceFrame(data) => (
                        reference_frame_transform(data),
                        data.perspective_matrix,
                        data.transform_style,
                        data.backface_visibility,
                        data.flattens_descendants,
                    ),
                    SpatialNodeSemantics::Scroll(data) => (
                        translation_matrix(
                            -(data.scroll_offset.x as f32),
                            -(data.scroll_offset.y as f32),
                        ),
                        None,
                        MpTransformStyle::Flat,
                        MpBackfaceVisibility::Visible,
                        true,
                    ),
                    SpatialNodeSemantics::Sticky(data) => {
                        let offset = sticky_used_offset(data);
                        (
                            translation_matrix(offset.x as f32, offset.y as f32),
                            None,
                            MpTransformStyle::Flat,
                            MpBackfaceVisibility::Visible,
                            true,
                        )
                    }
                    SpatialNodeSemantics::Root | SpatialNodeSemantics::IFrameRoot => (
                        Mat4f::identity(),
                        None,
                        MpTransformStyle::Flat,
                        MpBackfaceVisibility::Visible,
                        true,
                    ),
                }
            };
        MpReferenceFrame {
            parent: Some(parent),
            clip: None,
            local_rect: bounds,
            transform,
            perspective,
            transform_style,
            backface_visibility,
            flattens_descendants,
        }
    }

    fn effect_semantics_for_boundary(
        &self,
        parent_paint_container_id: PaintContainerId,
        paint_container_id: PaintContainerId,
    ) -> Option<crate::render_plan::NodeRenderSemantics> {
        let owner_node_id = self.render_scene.frame_owner_node_id(paint_container_id)?;
        if self.render_scene.frame_owner_node_id(parent_paint_container_id) == Some(owner_node_id) {
            return None;
        }
        let semantics = self.render_scene.owner_semantics(owner_node_id)?;
        if semantics.opacity == 1.0
            && !semantics.needs_isolation
            && !semantics.needs_filter
            && !semantics.needs_blend
            && !semantics.needs_mask
        {
            return None;
        }
        Some(semantics)
    }

    fn lower_item_run(
        &mut self,
        cx: &mut Cx2d,
        state: &mut MakepadDrawState<'_>,
        scene: &mut MpScene,
        parent: MpNodeId,
        paint_container_id: PaintContainerId,
        run_index: usize,
        item_indices: &[usize],
    ) {
        let Some(bounds) = self.item_run_bounds(paint_container_id, item_indices) else {
            return;
        };
        if bounds.size.x <= 0.0 || bounds.size.y <= 0.0 {
            return;
        }
        let texture = self.paint_item_run_surface(cx, state, paint_container_id, run_index, item_indices, bounds);
        scene.push(MpNode::Surface(MpSurfaceNode {
            parent,
            clip: None,
            local_rect: bounds,
            source: MpSurfaceSource::SurfaceTexture(texture),
            backface_visibility: MpBackfaceVisibility::Visible,
        }));
    }

    fn paint_item_run_surface(
        &mut self,
        cx: &mut Cx2d,
        state: &mut MakepadDrawState<'_>,
        paint_container_id: PaintContainerId,
        run_index: usize,
        item_indices: &[usize],
        bounds: Rect,
    ) -> Texture {
        let key = SceneSurfaceKey {
            paint_container_id,
            run_index,
        };
        let mut entry = state
            .frame_draw_lists
            .surfaces
            .remove(&key)
            .unwrap_or_else(|| SceneSurfaceCacheEntry {
                surface: MpSurface::new(
                    cx.cx,
                    bounds.size,
                    MpSurfaceColorFormat::BgraU8,
                    false,
                ),
                draw_list: DrawList2d::new(cx.cx),
            });
        entry.surface.resize(cx.cx, bounds.size);
        entry.surface.begin(cx, None);
        cx.set_pass_shift_scale(entry.surface.pass(), dvec2(0.0, 0.0), dvec2(1.0, 1.0));
        entry.draw_list.begin_always(cx);
        cx.begin_root_turtle_for_pass(Layout::default());
        for item_index in item_indices {
            let item = &self.render_scene.frame_items(paint_container_id)[*item_index];
            let local_origin =
                item_origin_in_paint_container(self.render_scene, item, paint_container_id)
                    - bounds.pos;
            let pushed = push_local_clip_chain_shifted(
                cx,
                self.render_scene,
                paint_container_id,
                item.clip_id,
                bounds.pos,
            );
            paint_fragment_item(cx, item, local_origin, state, 1.0);
            pop_clip_chain(cx, pushed);
        }
        cx.end_pass_sized_turtle();
        entry.draw_list.end(cx);
        entry.surface.end(cx);
        let texture = entry.surface.color_texture().clone();
        state.frame_draw_lists.surfaces.insert(key, entry);
        texture
    }

    fn lower_clip_chain(
        &mut self,
        scene: &mut MpScene,
        clip_id: SceneClipId,
        owner_parent: MpNodeId,
    ) -> Option<MpNodeId> {
        if clip_id == SceneClipId::INVALID {
            return None;
        }
        if let Some(existing) = self.clip_map.get(&clip_id) {
            return Some(*existing);
        }
        let clip = self.render_scene.clip_node(clip_id)?;
        let prev = self.lower_clip_chain(scene, clip.parent_clip_id, owner_parent);
        let geometry = self
            .render_scene
            .clip_geometry_in_paint_container(clip_id, self.root_paint_container_id)?;
        let shape = match geometry {
            SceneClipGeometry::Rect { rect } => MpClipShape::Rect { rect },
            SceneClipGeometry::RoundedRect { rect, radius } => {
                MpClipShape::RoundedRect { rect, radius }
            }
            SceneClipGeometry::PlaneSet { planes, count } => MpClipShape::PlaneSet {
                planes: planes[..count].to_vec(),
            },
            SceneClipGeometry::DeferredMask { rect } => MpClipShape::Rect { rect },
        };
        let node_id = scene.push(MpNode::Clip(MpClipNode {
            parent: Some(owner_parent),
            prev,
            shape,
        }));
        self.clip_map.insert(clip_id, node_id);
        Some(node_id)
    }

    fn item_run_bounds(
        &mut self,
        paint_container_id: PaintContainerId,
        item_indices: &[usize],
    ) -> Option<Rect> {
        item_indices
            .iter()
            .filter_map(|index| {
                frame_paint_item_local_rect(
                    self.render_scene,
                    &self.render_scene.frame_items(paint_container_id)[*index],
                )
            })
            .fold(None, union_rect)
    }

    fn paint_container_local_bounds(
        &mut self,
        paint_container_id: PaintContainerId,
    ) -> Option<Rect> {
        if let Some(cached) = self.bounds_cache.get(&paint_container_id) {
            return *cached;
        }
        let mut bounds = None;
        for command in self.render_scene.frame_paint_list(paint_container_id).iter().copied() {
            match command {
                ScenePaintCommand::Item(item_index) => {
                    if let Some(rect) = frame_paint_item_local_rect(
                        self.render_scene,
                        &self.render_scene.frame_items(paint_container_id)[item_index],
                    ) {
                        bounds = union_rect(bounds, rect);
                    }
                }
                ScenePaintCommand::ChildPaintContainer(child_paint_container_id) => {
                    if let Some(child_bounds) = self.paint_container_local_bounds(child_paint_container_id) {
                        bounds = union_rect(
                            bounds,
                            map_rect_between_paint_containers(
                                self.render_scene,
                                child_paint_container_id,
                                paint_container_id,
                                child_bounds,
                            ),
                        );
                    }
                }
            }
        }
        self.bounds_cache.insert(paint_container_id, bounds);
        bounds
    }
}

fn reference_frame_transform(data: crate::scene::ReferenceFrameData) -> Mat4f {
    let mut transform = translation_matrix(
        data.placement_origin.x as f32,
        data.placement_origin.y as f32,
    );
    if let Some(matrix) = data.transform_matrix {
        transform = Mat4f::mul(&transform, &matrix);
    }
    transform
}

fn sticky_used_offset(data: crate::scene::StickyNodeData) -> DVec2 {
    if data.margins.top.is_none()
        && data.margins.right.is_none()
        && data.margins.bottom.is_none()
        && data.margins.left.is_none()
    {
        return dvec2(0.0, 0.0);
    }

    let mut sticky_rect = data.frame_rect;
    let mut sticky_offset = dvec2(0.0, 0.0);

    if let Some(margin) = data.margins.top {
        let top_viewport_edge = data.scroll_port_rect.pos.y + margin as f64;
        if sticky_rect.pos.y < top_viewport_edge {
            sticky_offset.y = top_viewport_edge - sticky_rect.pos.y;
        }
    }

    if sticky_offset.y <= 0.0 {
        if let Some(margin) = data.margins.bottom {
            sticky_rect.pos.y += sticky_offset.y;
            let bottom_viewport_edge =
                data.scroll_port_rect.pos.y + data.scroll_port_rect.size.y - margin as f64;
            let sticky_bottom = sticky_rect.pos.y + sticky_rect.size.y;
            if sticky_bottom > bottom_viewport_edge {
                sticky_offset.y += bottom_viewport_edge - sticky_bottom;
            }
        }
    }

    if let Some(margin) = data.margins.left {
        let left_viewport_edge = data.scroll_port_rect.pos.x + margin as f64;
        if sticky_rect.pos.x < left_viewport_edge {
            sticky_offset.x = left_viewport_edge - sticky_rect.pos.x;
        }
    }

    if sticky_offset.x <= 0.0 {
        if let Some(margin) = data.margins.right {
            sticky_rect.pos.x += sticky_offset.x;
            let right_viewport_edge =
                data.scroll_port_rect.pos.x + data.scroll_port_rect.size.x - margin as f64;
            let sticky_right = sticky_rect.pos.x + sticky_rect.size.x;
            if sticky_right > right_viewport_edge {
                sticky_offset.x += right_viewport_edge - sticky_right;
            }
        }
    }

    sticky_offset.y = sticky_offset
        .y
        .max(data.vertical_offset_bounds.min as f64)
        .min(data.vertical_offset_bounds.max as f64);
    sticky_offset.x = sticky_offset
        .x
        .max(data.horizontal_offset_bounds.min as f64)
        .min(data.horizontal_offset_bounds.max as f64);

    let frame_left = data.frame_rect.pos.x;
    let frame_top = data.frame_rect.pos.y;
    let frame_right = data.frame_rect.pos.x + data.frame_rect.size.x;
    let frame_bottom = data.frame_rect.pos.y + data.frame_rect.size.y;
    let cb_left = data.containing_block_rect.pos.x;
    let cb_top = data.containing_block_rect.pos.y;
    let cb_right = data.containing_block_rect.pos.x + data.containing_block_rect.size.x;
    let cb_bottom = data.containing_block_rect.pos.y + data.containing_block_rect.size.y;
    sticky_offset.x = sticky_offset.x.max(cb_left - frame_left).min(cb_right - frame_right);
    sticky_offset.y = sticky_offset.y.max(cb_top - frame_top).min(cb_bottom - frame_bottom);

    dvec2(sticky_offset.x, sticky_offset.y)
}

fn translation_matrix(tx: f32, ty: f32) -> Mat4f {
    Mat4f {
        v: [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            tx, ty, 0.0, 1.0,
        ],
    }
}

fn frame_paint_item_local_rect(
    scene: &RenderScene<'_>,
    item: &ScenePaintItem<'_>,
) -> Option<Rect> {
    let item_origin = item_origin_in_paint_container(scene, item, item.owning_paint_container_id);
    let rect = match item.source {
        havi_fragment_semantics::Fragment::Box(bf)
        | havi_fragment_semantics::Fragment::Float(bf) => {
            let rect = bf.border_rect();
            Some(Rect {
                pos: dvec2(
                    item_origin.x + rect.origin.x.to_f32_px() as f64,
                    item_origin.y + rect.origin.y.to_f32_px() as f64,
                ),
                size: dvec2(
                    rect.size.width.to_f32_px() as f64,
                    rect.size.height.to_f32_px() as f64,
                ),
            })
        }
        havi_fragment_semantics::Fragment::Text(tf) => {
            let rect = tf.base.rect;
            Some(Rect {
                pos: dvec2(
                    item_origin.x + rect.origin.x.to_f32_px() as f64,
                    item_origin.y + rect.origin.y.to_f32_px() as f64,
                ),
                size: dvec2(
                    rect.size.width.to_f32_px() as f64,
                    rect.size.height.to_f32_px() as f64,
                ),
            })
        }
        havi_fragment_semantics::Fragment::Image(img) => {
            let rect = img.base.rect;
            Some(Rect {
                pos: dvec2(
                    item_origin.x + rect.origin.x.to_f32_px() as f64,
                    item_origin.y + rect.origin.y.to_f32_px() as f64,
                ),
                size: dvec2(
                    rect.size.width.to_f32_px() as f64,
                    rect.size.height.to_f32_px() as f64,
                ),
            })
        }
        havi_fragment_semantics::Fragment::IFrame(iframe) => {
            let rect = iframe.base.rect;
            Some(Rect {
                pos: dvec2(
                    item_origin.x + rect.origin.x.to_f32_px() as f64,
                    item_origin.y + rect.origin.y.to_f32_px() as f64,
                ),
                size: dvec2(
                    rect.size.width.to_f32_px() as f64,
                    rect.size.height.to_f32_px() as f64,
                ),
            })
        }
        havi_fragment_semantics::Fragment::Positioning(_)
        | havi_fragment_semantics::Fragment::AbsoluteOrFixedPositioned { .. } => None,
    };
    rect
}

fn item_origin_in_paint_container(
    scene: &RenderScene<'_>,
    item: &ScenePaintItem<'_>,
    target_paint_container_id: PaintContainerId,
) -> DVec2 {
    if item.owning_paint_container_id == target_paint_container_id {
        return item.local_origin;
    }
    let mapped = transform_point(
        &Mat4f::mul(
            &scene.frame_world_inverse(target_paint_container_id),
            &scene.frame_world_transform(item.owning_paint_container_id),
        ),
        item.local_origin,
    );
    dvec2(mapped.x, mapped.y)
}

fn push_local_clip_chain_shifted(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    paint_container_id: PaintContainerId,
    clip_id: SceneClipId,
    offset: DVec2,
) -> usize {
    if clip_id == SceneClipId::INVALID {
        return 0;
    }
    let mut chain = Vec::new();
    let mut current = clip_id;
    let paint_spatial_node_id = scene.paint_container_spatial_node_id(paint_container_id);
    while current != SceneClipId::INVALID {
        let node = scene.clip_node(current).unwrap();
        if node.spatial_node_id != paint_spatial_node_id {
            break;
        }
        chain.push(node.geometry);
        current = node.parent_clip_id;
    }
    chain.reverse();
    let pushed_count = chain.len();
    for geometry in &chain {
        match *geometry {
            SceneClipGeometry::Rect { rect }
            | SceneClipGeometry::RoundedRect { rect, .. }
            | SceneClipGeometry::DeferredMask { rect } => cx.push_clip_rect(Rect {
                pos: rect.pos - offset,
                size: rect.size,
            }),
            SceneClipGeometry::PlaneSet { .. } => {}
        }
    }
    pushed_count
}

fn pop_clip_chain(cx: &mut Cx2d, pushed_count: usize) {
    for _ in 0..pushed_count {
        cx.pop_clip_rect();
    }
}

fn map_rect_between_paint_containers(
    scene: &RenderScene<'_>,
    from_paint_container_id: PaintContainerId,
    to_paint_container_id: PaintContainerId,
    rect: Rect,
) -> Rect {
    if from_paint_container_id == to_paint_container_id {
        return rect;
    }
    let world_rect = transform_rect(&scene.frame_world_transform(from_paint_container_id), rect);
    transform_rect(&scene.frame_world_inverse(to_paint_container_id), world_rect)
}

fn transform_point(matrix: &Mat4f, point: DVec2) -> DVec2 {
    let mapped = matrix.transform_vec4(vec4f(point.x as f32, point.y as f32, 0.0, 1.0));
    if mapped.w.abs() > 1e-6 {
        dvec2((mapped.x / mapped.w) as f64, (mapped.y / mapped.w) as f64)
    } else {
        dvec2(mapped.x as f64, mapped.y as f64)
    }
}

fn transform_rect(matrix: &Mat4f, rect: Rect) -> Rect {
    let points = [
        dvec2(rect.pos.x, rect.pos.y),
        dvec2(rect.pos.x + rect.size.x, rect.pos.y),
        dvec2(rect.pos.x, rect.pos.y + rect.size.y),
        dvec2(rect.pos.x + rect.size.x, rect.pos.y + rect.size.y),
    ];
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for point in points {
        let mapped = transform_point(matrix, point);
        min_x = min_x.min(mapped.x);
        min_y = min_y.min(mapped.y);
        max_x = max_x.max(mapped.x);
        max_y = max_y.max(mapped.y);
    }
    Rect {
        pos: dvec2(min_x, min_y),
        size: dvec2((max_x - min_x).max(0.0), (max_y - min_y).max(0.0)),
    }
}

fn union_rect(current: Option<Rect>, next: Rect) -> Option<Rect> {
    match current {
        None => Some(next),
        Some(current) => {
            let min_x = current.pos.x.min(next.pos.x);
            let min_y = current.pos.y.min(next.pos.y);
            let max_x = (current.pos.x + current.size.x).max(next.pos.x + next.size.x);
            let max_y = (current.pos.y + current.size.y).max(next.pos.y + next.size.y);
            Some(Rect {
                pos: dvec2(min_x, min_y),
                size: dvec2((max_x - min_x).max(0.0), (max_y - min_y).max(0.0)),
            })
        }
    }
}
