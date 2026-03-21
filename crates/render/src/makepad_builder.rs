//! Paint traversal and composition for a pre-built render scene.

use std::collections::HashMap;
use std::env;

use makepad_compositor::{MpCompositedQuad, MpCompositor, MpSurface, MpSurfaceColorFormat};
use makepad_widgets::*;

use crate::compositor_scene::CompositorSurfaceId;
use crate::makepad_clip::{
    classify_clip_chain, map_rect_between_paint_containers, pop_clip_chain, push_clip_chain,
    push_local_clip_chain, transform_rect, ClipPushResult, MaskRuntime,
};
use crate::makepad_effects::{
    begin_filter_pass, begin_opacity_pass, end_filter_pass, end_opacity_pass, frame_effects_for_node,
};
use crate::makepad_fragments::{paint_fragment_item, paint_selection_overlay};
use crate::render_plan::RenderParticipation;
use crate::scene::{PaintContainerId, RenderScene, ScenePaintCommand, ScenePaintItem};
use crate::{
    BackendRootBasis, DrawBoxShadow, DrawFilterImage, DrawGradient, DrawRoundedColor,
    DrawVideoYuv, FilterState, FrameDrawListState, OpacityState, SelectionHighlight,
    TextureCache,
};

pub(crate) struct MakepadDrawState<'a> {
    pub draw_bg: &'a mut DrawColor,
    pub draw_text: &'a mut DrawText,
    pub draw_text_bold: &'a mut DrawText,
    pub draw_text_mono: &'a mut DrawText,
    pub draw_image: &'a mut DrawImage,
    pub texture_cache: &'a mut TextureCache,
    pub draw_rounded_bg: &'a mut DrawRoundedColor,
    pub draw_box_shadow: &'a mut DrawBoxShadow,
    pub draw_gradient: &'a mut DrawGradient,
    pub draw_video_yuv: &'a mut DrawVideoYuv,
    pub selection: Option<&'a SelectionHighlight>,
    pub opacity_state: &'a mut OpacityState,
    pub filter_state: &'a mut FilterState,
    pub draw_filter_image: &'a mut DrawFilterImage,
    #[allow(dead_code)]
    pub frame_draw_lists: &'a mut FrameDrawListState,
    pub image_overrides: &'a havi_types::ImageOverrides,
    pub(crate) active_container_transform: Mat4f,
}

impl<'a> MakepadDrawState<'a> {
    fn with_active_container_transform<R>(
        &mut self,
        transform: Mat4f,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let previous = self.active_container_transform;
        self.active_container_transform = transform;
        let result = f(self);
        self.active_container_transform = previous;
        result
    }

    pub(crate) fn active_container_transform(&self) -> Mat4f {
        self.active_container_transform
    }
}

struct CompositorTextureSurface {
    pass: DrawPass,
    color_texture: Texture,
    _depth_texture: Option<Texture>,
    size: DVec2,
    draw_list: DrawList2d,
    with_depth: bool,
}

struct CompositorRuntime {
    compositor: MpCompositor,
    surfaces: HashMap<CompositorSurfaceId, CompositorTextureSurface>,
}

struct BackendRuntime {
    compositor: CompositorRuntime,
    masks: MaskRuntime,
    mask_contents: HashMap<PaintContainerId, MpSurface>,
    debug_surface_identity_composite: bool,
    debug_surface_plain_paint: bool,
    debug_surface_trace: bool,
}

impl CompositorTextureSurface {
    fn new(cx: &mut Cx, with_depth: bool) -> Self {
        let initial_size = dvec2(1.0, 1.0);
        let pass = DrawPass::new(cx);
        let color_texture = Texture::new_with_format(
            cx,
            TextureFormat::RenderBGRAu8 {
                size: fixed_texture_size(initial_size),
                initial: true,
            },
        );
        pass.set_color_texture(
            cx,
            &color_texture,
            DrawPassClearColor::ClearWith(Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.0 }),
        );
        let depth_texture = with_depth.then(|| {
            let depth = Texture::new_with_format(
                cx,
                TextureFormat::DepthD32 {
                    size: fixed_texture_size(initial_size),
                    initial: true,
                },
            );
            pass.set_depth_texture(cx, &depth, DrawPassClearDepth::ClearWith(1.0));
            depth
        });
        pass.set_size(cx, initial_size);
        Self {
            pass,
            color_texture,
            _depth_texture: depth_texture,
            size: initial_size,
            draw_list: DrawList2d::new(cx),
            with_depth,
        }
    }

    fn resize(&mut self, cx: &mut Cx, size: DVec2) {
        let size = dvec2(size.x.max(1.0).ceil(), size.y.max(1.0).ceil());
        if self.size == size {
            return;
        }
        self.size = size;
        *self.color_texture.get_format(cx) = TextureFormat::RenderBGRAu8 {
            size: fixed_texture_size(size),
            initial: true,
        };
        self.pass.set_color_texture(
            cx,
            &self.color_texture,
            DrawPassClearColor::ClearWith(Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.0 }),
        );
        if let Some(depth_texture) = &self._depth_texture {
            *depth_texture.get_format(cx) = TextureFormat::DepthD32 {
                size: fixed_texture_size(size),
                initial: true,
            };
            self.pass
                .set_depth_texture(cx, depth_texture, DrawPassClearDepth::ClearWith(1.0));
        }
        self.pass.set_size(cx, size);
    }

    fn begin(&mut self, cx: &mut Cx2d, size: DVec2, shift: DVec2) {
        self.resize(cx.cx, size);
        cx.make_child_pass(&self.pass);
        cx.begin_pass(&self.pass, None);
        cx.set_pass_shift_scale(&self.pass, shift, dvec2(1.0, 1.0));
        self.draw_list.begin_always(cx);
    }

    fn end(&mut self, cx: &mut Cx2d) {
        self.draw_list.end(cx);
        cx.end_pass(&self.pass);
    }

    fn texture(&self) -> Texture {
        self.color_texture.clone()
    }
}

impl CompositorRuntime {
    fn new(cx: &mut Cx) -> Self {
        Self {
            compositor: MpCompositor::new(cx),
            surfaces: HashMap::new(),
        }
    }

}

impl BackendRuntime {
    fn new(cx: &mut Cx) -> Self {
        Self {
            compositor: CompositorRuntime::new(cx),
            masks: MaskRuntime::new(cx),
            mask_contents: HashMap::new(),
            debug_surface_identity_composite: env::var("HAVI_DEBUG_SURFACE_IDENTITY_COMPOSITE").ok().as_deref() == Some("1"),
            debug_surface_plain_paint: env::var("HAVI_DEBUG_SURFACE_PLAIN_PAINT").ok().as_deref() == Some("1"),
            debug_surface_trace: env::var("HAVI_DEBUG_SURFACE_TRACE").ok().as_deref() == Some("1"),
        }
    }

    fn ensure_mask_content_surface(&mut self, cx: &mut Cx, paint_container_id: PaintContainerId, size: DVec2) {
        self.mask_contents
            .entry(paint_container_id)
            .and_modify(|surface| surface.resize(cx, size))
            .or_insert_with(|| MpSurface::new(cx, size, MpSurfaceColorFormat::BgraU8, false));
    }

    fn begin_mask_content_surface(&mut self, cx: &mut Cx2d, paint_container_id: PaintContainerId, size: DVec2) {
        self.ensure_mask_content_surface(cx.cx, paint_container_id, size);
        let surface = self.mask_contents.get_mut(&paint_container_id).unwrap();
        surface.begin(cx, None);
        cx.set_pass_shift_scale(surface.pass(), dvec2(0.0, 0.0), dvec2(1.0, 1.0));
    }

    fn end_mask_content_surface(&mut self, cx: &mut Cx2d, paint_container_id: PaintContainerId) {
        self.mask_contents.get_mut(&paint_container_id).unwrap().end(cx);
    }

    fn mask_content_texture(&self, paint_container_id: PaintContainerId) -> Texture {
        self.mask_contents
            .get(&paint_container_id)
            .unwrap()
            .color_texture()
            .clone()
    }
}

impl CompositorRuntime {
    fn ensure_surface(
        &mut self,
        cx: &mut Cx,
        surface_id: CompositorSurfaceId,
        with_depth: bool,
    ) {
        let needs_recreate = self
            .surfaces
            .get(&surface_id)
            .is_some_and(|surface| surface.with_depth != with_depth);
        if needs_recreate {
            self.surfaces.remove(&surface_id);
        }
        self.surfaces
            .entry(surface_id)
            .or_insert_with(|| CompositorTextureSurface::new(cx, with_depth));
    }

    fn begin_surface(
        &mut self,
        cx: &mut Cx2d,
        surface_id: CompositorSurfaceId,
        size: DVec2,
        shift: DVec2,
        with_depth: bool,
    ) {
        self.ensure_surface(cx.cx, surface_id, with_depth);
        self.surfaces.get_mut(&surface_id).unwrap().begin(cx, size, shift);
    }

    fn end_surface(&mut self, cx: &mut Cx2d, surface_id: CompositorSurfaceId) {
        self.surfaces.get_mut(&surface_id).unwrap().end(cx);
    }

    fn surface_texture(&self, surface_id: CompositorSurfaceId) -> Texture {
        self.surfaces.get(&surface_id).unwrap().texture()
    }
}

pub(crate) fn paint_scene(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    backend_root_basis: BackendRootBasis,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    let mut runtime = BackendRuntime::new(cx.cx);
    paint_paint_container_target(
        cx,
        scene,
        &mut runtime,
        scene.root_paint_container_id(),
        None,
        scene.root_paint_container_id(),
        None,
        backend_root_basis,
        root_viewport_size,
        state,
        parent_opacity,
    );
    paint_selection_overlay(cx, state);
}

fn paint_paint_container_target(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    runtime: &mut BackendRuntime,
    paint_container_id: PaintContainerId,
    active_surface_id: Option<CompositorSurfaceId>,
    target_paint_container_id: PaintContainerId,
    space_root_paint_container_id: Option<PaintContainerId>,
    backend_root_basis: BackendRootBasis,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    let target_surface_id = scene.compositor_scene().frame_target_surface(paint_container_id);
    if target_surface_id != active_surface_id {
        if let Some(surface_id) = scene.frame_surface(paint_container_id) {
            paint_compositor_surface(
                cx,
                scene,
                runtime,
                surface_id,
                paint_container_id,
                target_paint_container_id,
                space_root_paint_container_id,
                backend_root_basis,
                root_viewport_size,
                state,
                parent_opacity,
            );
        }
        return;
    }

    match scene.frame_participation(paint_container_id) {
        RenderParticipation::Direct2d | RenderParticipation::Compositor { .. } => {
            paint_paint_container_direct_2d(
                cx,
                scene,
                runtime,
                paint_container_id,
                active_surface_id,
                target_paint_container_id,
                space_root_paint_container_id,
                backend_root_basis,
                root_viewport_size,
                state,
                parent_opacity,
            );
        }
    }
}

fn paint_compositor_surface(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    runtime: &mut BackendRuntime,
    surface_id: CompositorSurfaceId,
    surface_root_paint_container_id: PaintContainerId,
    target_paint_container_id: PaintContainerId,
    parent_space_root_paint_container_id: Option<PaintContainerId>,
    backend_root_basis: BackendRootBasis,
    _root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    let Some(local_bounds) = paint_container_subtree_bounds_in_space(
        scene,
        surface_root_paint_container_id,
        surface_root_paint_container_id,
    ) else {
        return;
    };
    if local_bounds.size.x <= 0.0 || local_bounds.size.y <= 0.0 {
        return;
    }

    let with_depth = matches!(
        scene.frame_participation(surface_root_paint_container_id),
        RenderParticipation::Compositor { .. }
    );
    runtime.compositor.begin_surface(cx, surface_id, local_bounds.size, dvec2(0.0, 0.0), with_depth);
    let surface_area = cx.current_pass_size();
    paint_paint_container_target(
        cx,
        scene,
        runtime,
        surface_root_paint_container_id,
        Some(surface_id),
        surface_root_paint_container_id,
        Some(surface_root_paint_container_id),
        backend_root_basis,
        surface_area,
        state,
        1.0,
    );
    runtime.compositor.end_surface(cx, surface_id);

    let composite_pass_size = cx.current_pass_size();
    let surface_texture = runtime.compositor.surface_texture(surface_id);
    if runtime.debug_surface_trace {
        eprintln!(
            "[havi][surface] composite surface_id={} root_frame={} target_frame={} active_parent_space_root={:?} local_bounds=({:.1},{:.1})+({:.1},{:.1}) child_pass_size=({:.1},{:.1}) parent_pass_size=({:.1},{:.1})",
            surface_id,
            surface_root_paint_container_id,
            target_paint_container_id,
            parent_space_root_paint_container_id,
            local_bounds.pos.x,
            local_bounds.pos.y,
            local_bounds.size.x,
            local_bounds.size.y,
            runtime.compositor.surfaces.get(&surface_id).map(|surface| surface.size.x).unwrap_or(0.0),
            runtime.compositor.surfaces.get(&surface_id).map(|surface| surface.size.y).unwrap_or(0.0),
            composite_pass_size.x,
            composite_pass_size.y,
        );
    }
    if runtime.debug_surface_identity_composite {
        let mut quad = MpCompositedQuad::new(
            surface_texture,
            Rect {
                pos: dvec2(20.0, 20.0),
                size: local_bounds.size,
            },
        );
        quad.transform = Mat4f::identity();
        quad.clip_planes.clear();
        quad.opacity = parent_opacity.clamp(0.0, 1.0);
        quad.depth_write = true;
        runtime.compositor.compositor.draw_quad(cx, &quad);
        return;
    }

    let projected_clip = projected_surface_clip(scene, target_paint_container_id, surface_root_paint_container_id);
    let frame_transform = paint_container_transform_in_space(
        scene,
        parent_space_root_paint_container_id,
        surface_root_paint_container_id,
    );
    let quad_transform = Mat4f::mul(
        &backend_root_basis.page_to_pass_transform(),
        &Mat4f::mul(
            &frame_transform,
            &translation_matrix(local_bounds.pos.x as f32, local_bounds.pos.y as f32),
        ),
    );

    let mut quad = MpCompositedQuad::new(
        surface_texture,
        Rect {
            pos: dvec2(0.0, 0.0),
            size: local_bounds.size,
        },
    );
    quad.transform = quad_transform;
    if runtime.debug_surface_trace {
        eprintln!(
            "[havi][surface] quad surface_id={} quad_rect=({:.1},{:.1})+({:.1},{:.1}) transform=[{:.3},{:.3},{:.3},{:.3};{:.3},{:.3},{:.3},{:.3};{:.3},{:.3},{:.3},{:.3};{:.3},{:.3},{:.3},{:.3}] projected_clip={}",
            surface_id,
            quad.local_rect.pos.x,
            quad.local_rect.pos.y,
            quad.local_rect.size.x,
            quad.local_rect.size.y,
            quad_transform.v[0], quad_transform.v[1], quad_transform.v[2], quad_transform.v[3],
            quad_transform.v[4], quad_transform.v[5], quad_transform.v[6], quad_transform.v[7],
            quad_transform.v[8], quad_transform.v[9], quad_transform.v[10], quad_transform.v[11],
            quad_transform.v[12], quad_transform.v[13], quad_transform.v[14], quad_transform.v[15],
            projected_clip.is_some(),
        );
    }
    if let Some(clip_planes) = projected_clip {
        quad.clip_planes = clip_planes.planes[..clip_planes.count].to_vec();
    }
    quad.opacity = parent_opacity.clamp(0.0, 1.0);
    quad.depth_write = true;
    runtime.compositor.compositor.draw_quad(cx, &quad);
}

fn paint_paint_container_direct_2d(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    runtime: &mut BackendRuntime,
    paint_container_id: PaintContainerId,
    active_surface_id: Option<CompositorSurfaceId>,
    target_paint_container_id: PaintContainerId,
    space_root_paint_container_id: Option<PaintContainerId>,
    backend_root_basis: BackendRootBasis,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    let active_container_transform = if paint_container_id == scene.root_paint_container_id() {
        Mat4f::identity()
    } else {
        paint_container_transform_in_space(
            scene,
            space_root_paint_container_id,
            paint_container_id,
        )
    };

    state.with_active_container_transform(active_container_transform, |state| {
        if paint_container_id == scene.root_paint_container_id() {
            cx.begin_page_root_turtle(backend_root_basis.webview_origin, root_viewport_size, Layout::default());
            state.draw_bg.color = vec4(1.0, 1.0, 1.0, 1.0);
            state.draw_bg.draw_abs(
                cx,
                Rect {
                    pos: backend_root_basis.webview_origin,
                    size: root_viewport_size,
                },
            );
            paint_paint_container_with_effects(
                cx,
                scene,
                runtime,
                paint_container_id,
                active_surface_id,
                target_paint_container_id,
                space_root_paint_container_id,
                backend_root_basis,
                root_viewport_size,
                state,
                parent_opacity,
            );
            cx.end_pass_sized_turtle();
            return;
        }

        if active_surface_id.is_some() {
            cx.begin_root_turtle_for_pass(Layout::default());
            let pass_size = cx.current_pass_size();
            if runtime.debug_surface_trace {
                eprintln!(
                    "[havi][surface] begin surface-local frame={} active_surface={:?} pass_size=({:.1},{:.1})",
                    paint_container_id,
                    active_surface_id,
                    pass_size.x,
                    pass_size.y,
                );
            }
            cx.turtle_mut().set_used(pass_size.x, pass_size.y);
            paint_paint_container_with_effects(
                cx,
                scene,
                runtime,
                paint_container_id,
                active_surface_id,
                target_paint_container_id,
                None,
                backend_root_basis,
                root_viewport_size,
                state,
                parent_opacity,
            );
            cx.end_pass_sized_turtle();
            return;
        }

        let pass_size = cx.current_pass_size();
        let frame_origin = backend_root_basis.page_to_pass_point(dvec2(0.0, 0.0));
        cx.begin_page_root_turtle(frame_origin, pass_size, Layout::default());
        paint_paint_container_with_effects(
            cx,
            scene,
            runtime,
            paint_container_id,
            active_surface_id,
            target_paint_container_id,
            space_root_paint_container_id,
            backend_root_basis,
            root_viewport_size,
            state,
            parent_opacity,
        );
        cx.end_pass_sized_turtle();
    });
}

fn should_use_surface_plain_paint(
    scene: &RenderScene<'_>,
    runtime: &BackendRuntime,
    paint_container_id: PaintContainerId,
    active_surface_id: Option<CompositorSurfaceId>,
) -> bool {
    if runtime.debug_surface_plain_paint && active_surface_id.is_some() {
        return true;
    }
    active_surface_id.is_some()
        && matches!(
            scene.frame_participation(paint_container_id),
            RenderParticipation::Compositor {
                group: crate::render_plan::CompositorGroupMode::Flat
            }
        )
}

fn paint_paint_container_with_effects(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    runtime: &mut BackendRuntime,
    paint_container_id: PaintContainerId,
    active_surface_id: Option<CompositorSurfaceId>,
    target_paint_container_id: PaintContainerId,
    space_root_paint_container_id: Option<PaintContainerId>,
    backend_root_basis: BackendRootBasis,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    if should_use_surface_plain_paint(scene, runtime, paint_container_id, active_surface_id) {
        paint_paint_container_contents(
            cx,
            scene,
            runtime,
            paint_container_id,
            active_surface_id,
            target_paint_container_id,
            space_root_paint_container_id,
            backend_root_basis,
            root_viewport_size,
            state,
            parent_opacity,
        );
        return;
    }
    let (element_opacity, css_filters) = frame_effects_for_node(scene, paint_container_id);
    let needs_filter = !css_filters.is_identity();
    let needs_opacity = element_opacity < 1.0 && !needs_filter;

    if paint_container_id != scene.root_paint_container_id() {
        if let Some((node_id, bounds)) = paint_container_owner_bounds_in_pass(scene, paint_container_id, space_root_paint_container_id, backend_root_basis) {
            let size = dvec2(bounds.size.x.max(1.0), bounds.size.y.max(1.0));
            if needs_filter {
                begin_filter_pass(cx, state, node_id, size, bounds.pos);
                paint_paint_container_contents(
                    cx,
                    scene,
                    runtime,
                    paint_container_id,
                    active_surface_id,
                    target_paint_container_id,
                    space_root_paint_container_id,
                    backend_root_basis,
                    root_viewport_size,
                    state,
                    1.0,
                );
                end_filter_pass(
                    cx,
                    state,
                    node_id,
                    bounds,
                    parent_opacity * element_opacity * css_filters.filter_opacity,
                    &css_filters,
                );
                return;
            }
            if needs_opacity {
                begin_opacity_pass(cx, state, node_id, size, bounds.pos);
                paint_paint_container_contents(
                    cx,
                    scene,
                    runtime,
                    paint_container_id,
                    active_surface_id,
                    target_paint_container_id,
                    space_root_paint_container_id,
                    backend_root_basis,
                    root_viewport_size,
                    state,
                    1.0,
                );
                end_opacity_pass(cx, state, node_id, bounds, parent_opacity * element_opacity);
                return;
            }
        }
    }

    paint_paint_container_contents(
        cx,
        scene,
        runtime,
        paint_container_id,
        active_surface_id,
        target_paint_container_id,
        space_root_paint_container_id,
        backend_root_basis,
        root_viewport_size,
        state,
        parent_opacity * element_opacity,
    );
}

fn paint_paint_container_contents(
    cx: &mut Cx2d,
    scene: &RenderScene<'_>,
    runtime: &mut BackendRuntime,
    paint_container_id: PaintContainerId,
    active_surface_id: Option<CompositorSurfaceId>,
    target_paint_container_id: PaintContainerId,
    space_root_paint_container_id: Option<PaintContainerId>,
    backend_root_basis: BackendRootBasis,
    root_viewport_size: DVec2,
    state: &mut MakepadDrawState<'_>,
    opacity: f32,
) {
    let paint_list = scene.frame_paint_list(paint_container_id).to_vec();
    for command in paint_list {
        match command {
            ScenePaintCommand::Item(item_index) => {
                let item = &scene.frame_items(paint_container_id)[item_index];
                if should_use_surface_plain_paint(scene, runtime, paint_container_id, active_surface_id) {
                    paint_fragment_item(cx, item, state, opacity);
                } else {
                    let pushed = push_local_clip_chain(cx, scene, paint_container_id, item.clip_id);
                    paint_fragment_item(cx, item, state, opacity);
                    pop_clip_chain(cx, pushed);
                }
            }
            ScenePaintCommand::ChildPaintContainer(child_paint_container_id) => {
                let child_target_surface_id = scene.compositor_scene().frame_target_surface(child_paint_container_id);
                if child_target_surface_id != active_surface_id {
                    if child_target_surface_id.is_some() || scene.frame_surface(child_paint_container_id).is_some() {
                        paint_paint_container_target(
                            cx,
                            scene,
                            runtime,
                            child_paint_container_id,
                            active_surface_id,
                            target_paint_container_id,
                            space_root_paint_container_id,
                            backend_root_basis,
                            root_viewport_size,
                            state,
                            opacity,
                        );
                    }
                    continue;
                }
                if should_use_surface_plain_paint(scene, runtime, paint_container_id, active_surface_id) {
                    paint_paint_container_target(
                        cx,
                        scene,
                        runtime,
                        child_paint_container_id,
                        active_surface_id,
                        target_paint_container_id,
                        space_root_paint_container_id,
                        backend_root_basis,
                        root_viewport_size,
                        state,
                        opacity,
                    );
                    continue;
                }
                let pushed: ClipPushResult = push_clip_chain(
                    cx,
                    scene,
                    paint_container_id,
                    scene.effective_clip_chain_for_paint_container(child_paint_container_id),
                );
                if let Some(chain) = pushed.mask_chain.as_ref() {
                    if let Some(mask_texture) = runtime.masks.begin_mask_chain(cx, paint_container_id, chain) {
                        let mask_rect = Rect {
                            pos: dvec2(0.0, 0.0),
                            size: cx.current_pass_size(),
                        };
                        runtime.begin_mask_content_surface(cx, paint_container_id, mask_rect.size);
                        cx.begin_unclipped_root_turtle_for_pass(Layout::default());
                        paint_paint_container_target(
                            cx,
                            scene,
                            runtime,
                            child_paint_container_id,
                            active_surface_id,
                            target_paint_container_id,
                            space_root_paint_container_id,
                            backend_root_basis,
                            root_viewport_size,
                            state,
                            opacity,
                        );
                        cx.end_pass_sized_turtle_no_clip();
                        runtime.end_mask_content_surface(cx, paint_container_id);

                        state.draw_filter_image.draw_vars.set_texture(0, &runtime.mask_content_texture(paint_container_id));
                        state.draw_filter_image.draw_vars.set_texture(1, &mask_texture);
                        state.draw_filter_image.use_mask = 1.0;
                        state.draw_filter_image.opacity = 1.0;
                        state.draw_filter_image.blur_radius = 0.0;
                        state.draw_filter_image.brightness = 1.0;
                        state.draw_filter_image.contrast = 1.0;
                        state.draw_filter_image.grayscale = 0.0;
                        state.draw_filter_image.hue_rotate = 0.0;
                        state.draw_filter_image.invert = 0.0;
                        state.draw_filter_image.saturate = 1.0;
                        state.draw_filter_image.sepia = 0.0;
                        state.draw_filter_image.tex_size = Vec2f { x: mask_rect.size.x as f32, y: mask_rect.size.y as f32 };
                        state.draw_filter_image.draw_abs(cx, mask_rect);
                        state.draw_filter_image.use_mask = 0.0;
                    }
                    pop_clip_chain(cx, pushed.rect_pushes);
                    continue;
                }
                if pushed.projected_quad_clip_planes.is_none() {
                    paint_paint_container_target(
                        cx,
                        scene,
                        runtime,
                        child_paint_container_id,
                        active_surface_id,
                        target_paint_container_id,
                        space_root_paint_container_id,
                        backend_root_basis,
                        root_viewport_size,
                        state,
                        opacity,
                    );
                }
                pop_clip_chain(cx, pushed.rect_pushes);
            }
        }
    }
}

fn projected_surface_clip(
    scene: &RenderScene<'_>,
    target_paint_container_id: PaintContainerId,
    paint_container_id: PaintContainerId,
) -> Option<crate::scene::BackendClipPlanes> {
    let clip_id = scene.effective_clip_chain_for_paint_container(paint_container_id);
    classify_clip_chain(scene, target_paint_container_id, clip_id)
        .push_result
        .projected_quad_clip_planes
}

fn paint_container_transform_in_space(
    scene: &RenderScene<'_>,
    space_root_paint_container_id: Option<PaintContainerId>,
    paint_container_id: PaintContainerId,
) -> Mat4f {
    match space_root_paint_container_id {
        Some(space_root_paint_container_id) => Mat4f::mul(
            &scene.frame_world_inverse(space_root_paint_container_id),
            &scene.frame_world_transform(paint_container_id),
        ),
        None => scene.frame_world_transform(paint_container_id),
    }
}

fn paint_container_owner_bounds_in_pass(
    scene: &RenderScene<'_>,
    paint_container_id: PaintContainerId,
    space_root_paint_container_id: Option<PaintContainerId>,
    backend_root_basis: BackendRootBasis,
) -> Option<(usize, Rect)> {
    let owner_node_id = scene.frame_owner_node_id(paint_container_id)?;
    let transform = Mat4f::mul(
        &backend_root_basis.page_to_pass_transform(),
        &paint_container_transform_in_space(scene, space_root_paint_container_id, paint_container_id),
    );
    for item in scene.frame_items(paint_container_id) {
        if let Some(local_rect) = frame_paint_item_local_rect(item) {
            return Some((owner_node_id, transform_rect(&transform, local_rect)));
        }
    }
    None
}

fn frame_paint_item_local_rect(item: &ScenePaintItem<'_>) -> Option<Rect> {
    match item.source {
        havi_fragment_semantics::Fragment::Box(bf) | havi_fragment_semantics::Fragment::Float(bf) => {
            let rect = bf.border_rect();
            Some(Rect {
                pos: dvec2(
                    item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                    item.local_origin.y + rect.origin.y.to_f32_px() as f64,
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
                    item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                    item.local_origin.y + rect.origin.y.to_f32_px() as f64,
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
                    item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                    item.local_origin.y + rect.origin.y.to_f32_px() as f64,
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
                    item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                    item.local_origin.y + rect.origin.y.to_f32_px() as f64,
                ),
                size: dvec2(
                    rect.size.width.to_f32_px() as f64,
                    rect.size.height.to_f32_px() as f64,
                ),
            })
        }
        havi_fragment_semantics::Fragment::Positioning(_) | havi_fragment_semantics::Fragment::AbsoluteOrFixedPositioned { .. } => None,
    }
}

fn paint_container_subtree_bounds_in_space(
    scene: &RenderScene<'_>,
    space_root_paint_container_id: PaintContainerId,
    paint_container_id: PaintContainerId,
) -> Option<Rect> {
    let target_surface_id = scene.compositor_scene().frame_target_surface(paint_container_id);
    let mut bounds = None;
    let paint_list = scene.frame_paint_list(paint_container_id).to_vec();
    for command in paint_list {
        match command {
            ScenePaintCommand::Item(item_index) => {
                if let Some(local_rect) = frame_paint_item_local_rect(&scene.frame_items(paint_container_id)[item_index]) {
                    let mapped = transform_rect(
                        &paint_container_transform_in_space(scene, Some(space_root_paint_container_id), paint_container_id),
                        local_rect,
                    );
                    bounds = union_rect(bounds, mapped);
                }
            }
            ScenePaintCommand::ChildPaintContainer(child_paint_container_id) => {
                let child_target_surface_id = scene.compositor_scene().frame_target_surface(child_paint_container_id);
                let child_surface_id = scene.frame_surface(child_paint_container_id);
                if child_target_surface_id != target_surface_id && child_surface_id.is_none() {
                    continue;
                }
                let child_bounds = if let Some(child_surface_id) = child_surface_id {
                    if Some(child_surface_id) != target_surface_id {
                        paint_container_subtree_bounds_in_space(
                            scene,
                            child_paint_container_id,
                            child_paint_container_id,
                        )
                        .map(|rect| {
                            map_rect_between_paint_containers(
                                scene,
                                child_paint_container_id,
                                space_root_paint_container_id,
                                rect,
                            )
                        })
                    } else {
                        paint_container_subtree_bounds_in_space(
                            scene,
                            space_root_paint_container_id,
                            child_paint_container_id,
                        )
                    }
                } else {
                    paint_container_subtree_bounds_in_space(
                        scene,
                        space_root_paint_container_id,
                        child_paint_container_id,
                    )
                };
                if let Some(child_bounds) = child_bounds {
                    bounds = union_rect(bounds, child_bounds);
                }
            }
        }
    }
    bounds
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

fn fixed_texture_size(size: DVec2) -> TextureSize {
    TextureSize::Fixed {
        width: size.x.max(1.0).ceil() as usize,
        height: size.y.max(1.0).ceil() as usize,
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
