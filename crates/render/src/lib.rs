//! Render layout fragments using Makepad's native draw pipeline.
//!
//! Walks the fragment tree recursively and emits DrawColor / DrawText calls.
//! Overflow clipping uses Makepad's GPU clip pipeline via push/pop_clip_rect.

mod background;
mod hit_test;
mod makepad_builder;
mod paint_order;
pub mod shaders;
pub(crate) mod stacking_context;
mod text;
mod transform;

// Color helpers shared across modules.
pub(crate) mod color {
    use makepad_widgets::Vec4f;
    use style::color::{AbsoluteColor, ColorSpace};
    use style::properties::ComputedValues;

    pub fn abs_to_vec4(color: &AbsoluteColor) -> Vec4f {
        let srgb = color.to_color_space(ColorSpace::Srgb);
        Vec4f { x: srgb.components.0, y: srgb.components.1, z: srgb.components.2, w: srgb.alpha }
    }

    pub fn resolve_color(color: &style::values::computed::Color, current_color: &AbsoluteColor) -> Vec4f {
        abs_to_vec4(&color.resolve_to_absolute(current_color))
    }

    pub fn inherited_color(computed: &ComputedValues) -> Vec4f {
        abs_to_vec4(&computed.get_inherited_text().color)
    }
}

use std::collections::HashMap;

use havi_types::{Fragment, ImageFragment};
use makepad_widgets::*;
use makepad_widgets::makepad_draw::{ImageBuffer, Texture};
use makepad_widgets::makepad_draw::draw_list_2d::{DrawList2d, DrawListExt};
use app_units::Au;
use style::computed_values::position::T as ComputedPosition;
use style::values::computed::basic_shape::{BasicShape, ClipPath};
use style::values::computed::box_::Overflow;
use style::values::generics::basic_shape::GenericShapeRadius;
use style::values::generics::position::GenericPositionOrAuto;

pub use shaders::{DrawRoundedColor, DrawBoxShadow, DrawGradient, DrawFilterImage};
pub use hit_test::{hit_test, find_scroll_container};
pub use paint_order::paint_order;


use background::draw_element_box;
use text::draw_text_run;
use transform::{compute_css_transform_2d, compute_css_transform_3d, is_3d_matrix};

/// Cache for image textures, keyed by OpaqueNode id.
/// Call `clear()` when the fragment tree changes to avoid stale textures.
pub type TextureCache = HashMap<usize, Texture>;

/// Pre-computed selection highlight rectangles in content-space.
#[derive(Clone, Debug)]
pub struct SelectionHighlight {
    pub color: Vec4f,
    pub rects: Vec<Rect>,
}

/// Per-element scroll offsets for overflow containers, keyed by OpaqueNode id.
pub type ScrollState = HashMap<usize, DVec2>;

/// Reusable sub-DrawLists for elements with CSS transforms (rotation/skew).
/// Keyed by OpaqueNode id to allow reuse across frames.
pub type TransformState = HashMap<usize, DrawList2d>;

/// Render-to-texture state for opacity isolation (CSS stacking context).
/// When an element has opacity < 1.0, its subtree must be composited as a group
/// to avoid double-blending at overlapping child regions.
pub struct OpacityPass {
    pub pass: DrawPass,
    pub texture: Texture,
    pub draw_list: DrawList2d,
}

/// Per-element opacity passes, keyed by OpaqueNode id.
pub type OpacityState = HashMap<usize, OpacityPass>;

/// Render-to-texture state for CSS filter effects.
/// Like OpacityPass but composited with a filter shader.
pub struct FilterPass {
    pub pass: DrawPass,
    pub texture: Texture,
    pub draw_list: DrawList2d,
}

/// Per-element filter passes, keyed by OpaqueNode id.
pub type FilterState = HashMap<usize, FilterPass>;

/// CSS filter parameters resolved from computed values.
pub(crate) struct CssFilters {
    pub(crate) blur_radius: f32,
    pub(crate) brightness: f32,
    pub(crate) contrast: f32,
    pub(crate) grayscale: f32,
    pub(crate) hue_rotate_deg: f32,
    pub(crate) invert: f32,
    pub(crate) saturate: f32,
    pub(crate) sepia: f32,
    pub(crate) filter_opacity: f32,
}

impl CssFilters {
    pub(crate) fn identity() -> Self {
        Self {
            blur_radius: 0.0, brightness: 1.0, contrast: 1.0,
            grayscale: 0.0, hue_rotate_deg: 0.0, invert: 0.0,
            saturate: 1.0, sepia: 0.0, filter_opacity: 1.0,
        }
    }

    pub(crate) fn is_identity(&self) -> bool {
        self.blur_radius < 0.001
            && (self.brightness - 1.0).abs() < 0.001
            && (self.contrast - 1.0).abs() < 0.001
            && self.grayscale < 0.001
            && self.hue_rotate_deg.abs() < 0.001
            && self.invert < 0.001
            && (self.saturate - 1.0).abs() < 0.001
            && self.sepia < 0.001
            && (self.filter_opacity - 1.0).abs() < 0.001
    }
}

/// Extract CSS filter parameters from computed values.
pub(crate) fn resolve_css_filters(computed: &style::properties::ComputedValues) -> CssFilters {
    use style::values::computed::Filter;
    let effects = computed.get_effects();
    let mut f = CssFilters::identity();
    for filter in effects.filter.0.iter() {
        match *filter {
            Filter::Blur(ref radius) => f.blur_radius = radius.0.px(),
            Filter::Brightness(ref amount) => f.brightness = amount.0,
            Filter::Contrast(ref amount) => f.contrast = amount.0,
            Filter::Grayscale(ref amount) => f.grayscale = amount.0,
            Filter::HueRotate(angle) => f.hue_rotate_deg = angle.degrees(),
            Filter::Invert(ref amount) => f.invert = amount.0,
            Filter::Opacity(ref amount) => f.filter_opacity = amount.0,
            Filter::Saturate(ref amount) => f.saturate = amount.0,
            Filter::Sepia(ref amount) => f.sepia = amount.0,
            Filter::DropShadow(_) => {} // TODO: drop-shadow filter
            Filter::Url(_) => {}
        }
    }
    f
}



/// Draw all fragments from a layout result onto a Makepad 2D context.
pub fn render_fragments(
    cx: &mut Cx2d,
    fragments: &[Fragment],
    origin: DVec2,
    draw_bg: &mut DrawColor,
    draw_text: &mut DrawText,
    draw_text_bold: &mut DrawText,
    draw_text_mono: &mut DrawText,
    draw_image: &mut DrawImage,
    texture_cache: &mut TextureCache,
    scroll_state: &ScrollState,
    draw_rounded_bg: &mut DrawRoundedColor,
    draw_box_shadow: &mut DrawBoxShadow,
    draw_gradient: &mut DrawGradient,
    selection: Option<&SelectionHighlight>,
    transform_state: &mut TransformState,
    opacity_state: &mut OpacityState,
    filter_state: &mut FilterState,
    draw_filter_image: &mut DrawFilterImage,
) {
    for fragment in fragments {
        render_fragment(
            cx, fragment, origin, 0.0, None, 1.0,
            draw_bg, draw_text, draw_text_bold, draw_text_mono,
            draw_image, texture_cache, scroll_state,
            draw_rounded_bg, draw_box_shadow, draw_gradient, selection,
            transform_state, opacity_state, filter_state, draw_filter_image,
        );
    }
}

/// Draw fragments with viewport clipping.
pub fn render_fragments_clipped(
    cx: &mut Cx2d,
    fragments: &[Fragment],
    origin: DVec2,
    viewport_top: f32,
    viewport_bottom: f32,
    draw_bg: &mut DrawColor,
    draw_text: &mut DrawText,
    draw_text_bold: &mut DrawText,
    draw_text_mono: &mut DrawText,
    draw_image: &mut DrawImage,
    texture_cache: &mut TextureCache,
    scroll_state: &ScrollState,
    draw_rounded_bg: &mut DrawRoundedColor,
    draw_box_shadow: &mut DrawBoxShadow,
    draw_gradient: &mut DrawGradient,
    selection: Option<&SelectionHighlight>,
    transform_state: &mut TransformState,
    opacity_state: &mut OpacityState,
    filter_state: &mut FilterState,
    draw_filter_image: &mut DrawFilterImage,
) {
    for fragment in fragments {
        render_fragment(
            cx, fragment, origin, 0.0,
            Some((viewport_top, viewport_bottom)), 1.0,
            draw_bg, draw_text, draw_text_bold, draw_text_mono,
            draw_image, texture_cache, scroll_state,
            draw_rounded_bg, draw_box_shadow, draw_gradient, selection,
            transform_state, opacity_state, filter_state, draw_filter_image,
        );
    }
}

/// Render fragments using the stacking context tree for correct CSS paint ordering.
pub fn render_fragments_stacked(
    cx: &mut Cx2d,
    fragments: &[Fragment],
    origin: DVec2,
    draw_bg: &mut DrawColor,
    draw_text: &mut DrawText,
    draw_text_bold: &mut DrawText,
    draw_text_mono: &mut DrawText,
    draw_image: &mut DrawImage,
    texture_cache: &mut TextureCache,
    scroll_state: &ScrollState,
    draw_rounded_bg: &mut DrawRoundedColor,
    draw_box_shadow: &mut DrawBoxShadow,
    draw_gradient: &mut DrawGradient,
    selection: Option<&SelectionHighlight>,
    transform_state: &mut TransformState,
    opacity_state: &mut OpacityState,
    filter_state: &mut FilterState,
    draw_filter_image: &mut DrawFilterImage,
) {
    let sc = stacking_context::build_stacking_context_tree(fragments);
    let mut state = makepad_builder::MakepadDrawState {
        draw_bg, draw_text, draw_text_bold, draw_text_mono,
        draw_image, texture_cache, scroll_state, draw_rounded_bg,
        draw_box_shadow, draw_gradient, selection, transform_state,
        opacity_state, filter_state, draw_filter_image,
    };
    makepad_builder::paint_stacking_context(cx, &sc, origin, None, 1.0, &mut state);
}

/// Render fragments with viewport clipping using the stacking context tree.
pub fn render_fragments_stacked_clipped(
    cx: &mut Cx2d,
    fragments: &[Fragment],
    origin: DVec2,
    viewport_top: f32,
    viewport_bottom: f32,
    draw_bg: &mut DrawColor,
    draw_text: &mut DrawText,
    draw_text_bold: &mut DrawText,
    draw_text_mono: &mut DrawText,
    draw_image: &mut DrawImage,
    texture_cache: &mut TextureCache,
    scroll_state: &ScrollState,
    draw_rounded_bg: &mut DrawRoundedColor,
    draw_box_shadow: &mut DrawBoxShadow,
    draw_gradient: &mut DrawGradient,
    selection: Option<&SelectionHighlight>,
    transform_state: &mut TransformState,
    opacity_state: &mut OpacityState,
    filter_state: &mut FilterState,
    draw_filter_image: &mut DrawFilterImage,
) {
    let sc = stacking_context::build_stacking_context_tree(fragments);
    let mut state = makepad_builder::MakepadDrawState {
        draw_bg, draw_text, draw_text_bold, draw_text_mono,
        draw_image, texture_cache, scroll_state, draw_rounded_bg,
        draw_box_shadow, draw_gradient, selection, transform_state,
        opacity_state, filter_state, draw_filter_image,
    };
    makepad_builder::paint_stacking_context(
        cx, &sc, origin, Some((viewport_top, viewport_bottom)), 1.0, &mut state,
    );
}

fn render_fragment(
    cx: &mut Cx2d,
    fragment: &Fragment,
    parent_draw_origin: DVec2,
    parent_content_y: f32,
    clip: Option<(f32, f32)>,
    parent_opacity: f32,
    draw_bg: &mut DrawColor,
    draw_text: &mut DrawText,
    draw_text_bold: &mut DrawText,
    draw_text_mono: &mut DrawText,
    draw_image: &mut DrawImage,
    texture_cache: &mut TextureCache,
    scroll_state: &ScrollState,
    draw_rounded_bg: &mut DrawRoundedColor,
    draw_box_shadow: &mut DrawBoxShadow,
    draw_gradient: &mut DrawGradient,
    selection: Option<&SelectionHighlight>,
    transform_state: &mut TransformState,
    opacity_state: &mut OpacityState,
    filter_state: &mut FilterState,
    draw_filter_image: &mut DrawFilterImage,
) {
    let rect = fragment.content_rect();
    let x = parent_draw_origin.x + rect.origin.x.to_f32_px() as f64;
    let y = parent_draw_origin.y + rect.origin.y.to_f32_px() as f64;
    let w = rect.size.width.to_f32_px();
    let h = rect.size.height.to_f32_px();
    let content_y = parent_content_y + rect.origin.y.to_f32_px();

    if let Some((top, bottom)) = clip {
        if content_y + h < top || content_y > bottom { return; }
    }

    let element_opacity = match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => bf.base.style.get_effects().opacity,
        _ => 1.0,
    };
    // CSS filters (blur, brightness, contrast, etc).
    let css_filters = match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => resolve_css_filters(&bf.base.style),
        _ => CssFilters::identity(),
    };
    let needs_filter_pass = !css_filters.is_identity();

    let needs_opacity_isolation = element_opacity < 1.0 && !needs_filter_pass;
    // When isolated, children render at full opacity inside the pass; the pass
    // texture is then composited with the combined opacity.
    // If filters are active, they handle opacity too.
    let opacity = if needs_opacity_isolation || needs_filter_pass { 1.0 } else { parent_opacity * element_opacity };

    // CSS transform: determine translation offset and optional view_transform matrix.
    // Uses the 3D path when perspective or 3D transform operations are present.
    let (tx, ty, view_mat) = match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            let br = bf.border_rect();
            let bw = br.size.width.to_f32_px();
            let bh = br.size.height.to_f32_px();
            let t2d = compute_css_transform_2d(&bf.base.style, bw, bh);
            let t3d = compute_css_transform_3d(&bf.base.style, bw, bh);
            if t3d.as_ref().map_or(false, is_3d_matrix) {
                let mat = t3d.unwrap();
                (mat[12], mat[13], Some(mat))
            } else {
                match t2d {
                    Some(t) if !t.is_translate_only() => (t.tx, t.ty, Some(t.to_mat4f())),
                    Some(t) => (t.tx, t.ty, None),
                    None => (0.0, 0.0, None),
                }
            }
        }
        _ => (0.0, 0.0, None),
    };
    let has_non_trivial_transform = view_mat.is_some();

    // For elements with rotation/skew, wrap rendering in a sub-DrawList with view_transform.
    let node_id = fragment.tag().map(|t| t.node.0);
    if has_non_trivial_transform {
        if let Some(node_id) = node_id {
            let dl = transform_state.entry(node_id)
                .or_insert_with(|| DrawList2d::new(cx.cx));
            dl.begin_always(cx);
        }
    }

    // Sticky positioning: compute visual offset based on scroll position.
    let (sticky_dx, sticky_dy) = compute_sticky_offset(fragment, parent_draw_origin, clip);

    let x = x + tx as f64 + sticky_dx;
    let y = y + ty as f64 + sticky_dy;

    // Clip-path: render subtree into texture, composite through SDF clip shader.
    let clip_path_shape = match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            match bf.base.style.clone_clip_path() {
                ClipPath::Shape(shape, _) => Some(*shape),
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(ref shape) = clip_path_shape {
        let (bx, by, bw, bh) = match fragment {
            Fragment::Box(bf) | Fragment::Float(bf) => {
                let br = bf.border_rect();
                (
                    parent_draw_origin.x + br.origin.x.to_f32_px() as f64 + tx as f64 + sticky_dx,
                    parent_draw_origin.y + br.origin.y.to_f32_px() as f64 + ty as f64 + sticky_dy,
                    br.size.width.to_f32_px() as f64,
                    br.size.height.to_f32_px() as f64,
                )
            }
            _ => (x, y, w as f64, h as f64),
        };
        // Use Makepad's native SDF clip primitives instead of render-to-texture.
        let clip_rect = Rect { pos: dvec2(bx, by), size: dvec2(bw, bh) };
        push_clip_shape(cx, shape, clip_rect, bw as f32, bh as f32);
    }

    // CSS filter pass: render subtree to texture, composite with filter shader.
    // Handles blur, brightness, contrast, grayscale, hue-rotate, invert,
    // saturate, sepia, and filter opacity. Also applies element opacity.
    if needs_filter_pass {
        if let Some(node_id) = node_id {
            let (bx, by, bw, bh) = element_border_box(fragment, parent_draw_origin, tx, ty, sticky_dx, sticky_dy, x, y, w, h);
            let pw = bw.max(1.0);
            let ph = bh.max(1.0);

            let fp = filter_state.entry(node_id).or_insert_with(|| {
                let pass = DrawPass::new(cx.cx);
                let texture = Texture::new_with_format(cx.cx, TextureFormat::RenderBGRAu8 {
                    size: TextureSize::Auto,
                    initial: true,
                });
                pass.set_color_texture(cx.cx, &texture, DrawPassClearColor::ClearWith(
                    Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.0 },
                ));
                let draw_list = DrawList2d::new(cx.cx);
                FilterPass { pass, texture, draw_list }
            });
            fp.pass.set_size(cx.cx, dvec2(pw, ph));
            cx.make_child_pass(&fp.pass);
            cx.begin_pass(&fp.pass, None);
            cx.set_pass_shift_scale(&fp.pass, dvec2(bx, by), dvec2(1.0, 1.0));
            fp.draw_list.begin_always(cx);

            render_fragment_content(
                cx, fragment, parent_draw_origin, x, y, w, h, tx, ty,
                sticky_dx, sticky_dy,
                content_y, clip, 1.0,
                draw_bg, draw_text, draw_text_bold, draw_text_mono,
                draw_image, texture_cache, scroll_state,
                draw_rounded_bg, draw_box_shadow, draw_gradient, selection,
                transform_state, opacity_state, filter_state, draw_filter_image,
            );

            let fp = filter_state.get_mut(&node_id).unwrap();
            fp.draw_list.end(cx);
            cx.end_pass(&fp.pass);

            // Composite with filter shader.
            let combined_opacity = parent_opacity * element_opacity * css_filters.filter_opacity;
            draw_filter_image.draw_vars.set_texture(0, &fp.texture);
            draw_filter_image.opacity = combined_opacity;
            draw_filter_image.blur_radius = css_filters.blur_radius;
            draw_filter_image.brightness = css_filters.brightness;
            draw_filter_image.contrast = css_filters.contrast;
            draw_filter_image.grayscale = css_filters.grayscale;
            draw_filter_image.hue_rotate = css_filters.hue_rotate_deg;
            draw_filter_image.invert = css_filters.invert;
            draw_filter_image.saturate = css_filters.saturate;
            draw_filter_image.sepia = css_filters.sepia;
            draw_filter_image.tex_size = Vec2f { x: pw as f32, y: ph as f32 };
            draw_filter_image.draw_abs(cx, Rect {
                pos: dvec2(bx, by),
                size: dvec2(pw, ph),
            });

            close_transform_and_clip(cx, has_non_trivial_transform, node_id, transform_state, &view_mat, clip_path_shape.is_some());
            return;
        }
    }

    // Opacity isolation: render subtree into an offscreen pass at full opacity,
    // then composite the texture with the element's opacity.  This prevents
    // double-blending where children overlap (CSS compositing spec §3).
    if needs_opacity_isolation {
        if let Some(node_id) = node_id {
            let (bx, by, bw, bh) = element_border_box(fragment, parent_draw_origin, tx, ty, sticky_dx, sticky_dy, x, y, w, h);
            let pw = bw.max(1.0);
            let ph = bh.max(1.0);

            let op = opacity_state.entry(node_id).or_insert_with(|| {
                let pass = DrawPass::new(cx.cx);
                let texture = Texture::new_with_format(cx.cx, TextureFormat::RenderBGRAu8 {
                    size: TextureSize::Auto,
                    initial: true,
                });
                pass.set_color_texture(cx.cx, &texture, DrawPassClearColor::ClearWith(
                    Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.0 },
                ));
                let draw_list = DrawList2d::new(cx.cx);
                OpacityPass { pass, texture, draw_list }
            });
            op.pass.set_size(cx.cx, dvec2(pw, ph));
            cx.make_child_pass(&op.pass);
            cx.begin_pass(&op.pass, None);
            cx.set_pass_shift_scale(&op.pass, dvec2(bx, by), dvec2(1.0, 1.0));
            op.draw_list.begin_always(cx);

            render_fragment_content(
                cx, fragment, parent_draw_origin, x, y, w, h, tx, ty,
                sticky_dx, sticky_dy,
                content_y, clip, 1.0,
                draw_bg, draw_text, draw_text_bold, draw_text_mono,
                draw_image, texture_cache, scroll_state,
                draw_rounded_bg, draw_box_shadow, draw_gradient, selection,
                transform_state, opacity_state, filter_state, draw_filter_image,
            );

            let op = opacity_state.get_mut(&node_id).unwrap();
            op.draw_list.end(cx);
            cx.end_pass(&op.pass);

            let combined_opacity = parent_opacity * element_opacity;
            draw_image.draw_vars.set_texture(0, &op.texture);
            draw_image.opacity = combined_opacity;
            draw_image.draw_abs(cx, Rect {
                pos: dvec2(bx, by),
                size: dvec2(pw, ph),
            });

            close_transform_and_clip(cx, has_non_trivial_transform, node_id, transform_state, &view_mat, clip_path_shape.is_some());
            return;
        }
    }

    render_fragment_content(
        cx, fragment, parent_draw_origin, x, y, w, h, tx, ty,
        sticky_dx, sticky_dy,
        content_y, clip, opacity,
        draw_bg, draw_text, draw_text_bold, draw_text_mono,
        draw_image, texture_cache, scroll_state,
        draw_rounded_bg, draw_box_shadow, draw_gradient, selection,
        transform_state, opacity_state, filter_state, draw_filter_image,
    );

    // Close sub-DrawList and apply view_transform for rotation/skew/3D.
    if has_non_trivial_transform {
        if let Some(node_id) = node_id {
            if let Some(dl) = transform_state.get_mut(&node_id) {
                dl.end(cx);
                let mat = Mat4f { v: view_mat.unwrap() };
                dl.set_view_transform(cx.cx, &mat);
            }
        }
    }

    // Pop clip shape if one was pushed.
    if clip_path_shape.is_some() { cx.pop_clip_shape(); }
}

/// Compute the element's border box in screen space.
pub(crate) fn element_border_box(
    fragment: &Fragment, parent_draw_origin: DVec2,
    tx: f32, ty: f32, sticky_dx: f64, sticky_dy: f64,
    x: f64, y: f64, w: f32, h: f32,
) -> (f64, f64, f64, f64) {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            let br = bf.border_rect();
            (
                parent_draw_origin.x + br.origin.x.to_f32_px() as f64 + tx as f64 + sticky_dx,
                parent_draw_origin.y + br.origin.y.to_f32_px() as f64 + ty as f64 + sticky_dy,
                br.size.width.to_f32_px() as f64,
                br.size.height.to_f32_px() as f64,
            )
        }
        _ => (x, y, w as f64, h as f64),
    }
}

/// Close transform sub-DrawList and pop clip shape.
fn close_transform_and_clip(
    cx: &mut Cx2d,
    has_non_trivial_transform: bool,
    node_id: usize,
    transform_state: &mut TransformState,
    view_mat: &Option<[f32; 16]>,
    has_clip_path: bool,
) {
    if has_non_trivial_transform {
        if let Some(dl) = transform_state.get_mut(&node_id) {
            dl.end(cx);
            let mat = Mat4f { v: view_mat.unwrap() };
            dl.set_view_transform(cx.cx, &mat);
        }
    }
    if has_clip_path { cx.pop_clip_shape(); }
}

/// Draws the element's box, text, image, and children. Factored out of
/// `render_fragment` so the opacity-isolation path can call it inside a pass.
#[allow(clippy::too_many_arguments)]
fn render_fragment_content(
    cx: &mut Cx2d,
    fragment: &Fragment,
    parent_draw_origin: DVec2,
    x: f64, y: f64, w: f32, h: f32,
    tx: f32, ty: f32,
    sticky_dx: f64, sticky_dy: f64,
    content_y: f32,
    clip: Option<(f32, f32)>,
    opacity: f32,
    draw_bg: &mut DrawColor,
    draw_text: &mut DrawText,
    draw_text_bold: &mut DrawText,
    draw_text_mono: &mut DrawText,
    draw_image: &mut DrawImage,
    texture_cache: &mut TextureCache,
    scroll_state: &ScrollState,
    draw_rounded_bg: &mut DrawRoundedColor,
    draw_box_shadow: &mut DrawBoxShadow,
    draw_gradient: &mut DrawGradient,
    selection: Option<&SelectionHighlight>,
    transform_state: &mut TransformState,
    opacity_state: &mut OpacityState,
    filter_state: &mut FilterState,
    draw_filter_image: &mut DrawFilterImage,
) {
    match fragment {
        Fragment::Box(box_fragment) | Fragment::Float(box_fragment) => {
            let border_rect = box_fragment.border_rect();
            let bx = parent_draw_origin.x + border_rect.origin.x.to_f32_px() as f64 + tx as f64 + sticky_dx;
            let by = parent_draw_origin.y + border_rect.origin.y.to_f32_px() as f64 + ty as f64 + sticky_dy;
            let bw = border_rect.size.width.to_f32_px();
            let bh = border_rect.size.height.to_f32_px();
            draw_element_box(cx, &box_fragment.base.style, bx, by, bw, bh,
                draw_bg, draw_rounded_bg, draw_box_shadow, draw_gradient, opacity);
        }
        Fragment::Text(text_fragment) => {
            if let Some(sel) = selection {
                let text_rect = Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) };
                for sel_rect in &sel.rects {
                    if rects_overlap(&text_rect, sel_rect) {
                        draw_bg.color = sel.color;
                        draw_bg.draw_abs(cx, text_rect);
                        break;
                    }
                }
            }
            draw_text_run(cx, text_fragment, x, y, w, h, opacity,
                draw_bg, draw_text, draw_text_bold, draw_text_mono);
        }
        Fragment::Image(image_fragment) => {
            draw_image_fragment(cx, image_fragment, x, y, w, h, draw_image, texture_cache, opacity);
        }
        Fragment::IFrame(iframe) => {
            // Clip to iframe rect and render child fragment tree.
            let iframe_rect = Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) };
            cx.push_clip_rect(iframe_rect);
            for child in iframe.child_fragments.iter() {
                render_fragment(cx, child, dvec2(x, y), content_y, clip,
                    opacity, draw_bg, draw_text, draw_text_bold, draw_text_mono,
                    draw_image, texture_cache, scroll_state, draw_rounded_bg,
                    draw_box_shadow, draw_gradient, selection, transform_state,
                    opacity_state, filter_state, draw_filter_image);
            }
            cx.pop_clip_rect();
        }
        Fragment::Positioning(_) => {}
    }

    if let Some(children) = fragment.children() {
        let needs_clip = match fragment {
            Fragment::Box(bf) | Fragment::Float(bf) => {
                let ov = bf.base.style.get_box();
                !matches!(ov.overflow_x, Overflow::Visible) || !matches!(ov.overflow_y, Overflow::Visible)
            }
            _ => false,
        };

        let mut used_rounded_clip = false;
        if needs_clip {
            let bf = match fragment { Fragment::Box(bf) | Fragment::Float(bf) => bf, _ => unreachable!() };
            let border_rect = bf.border_rect();
            let bx = parent_draw_origin.x + border_rect.origin.x.to_f32_px() as f64 + tx as f64 + sticky_dx;
            let by = parent_draw_origin.y + border_rect.origin.y.to_f32_px() as f64 + ty as f64 + sticky_dy;
            let clip_rect = Rect {
                pos: dvec2(bx, by),
                size: dvec2(border_rect.size.width.to_f32_px() as f64, border_rect.size.height.to_f32_px() as f64),
            };
            let radii = background::resolve_border_radii(&bf.base.style);
            if radii.max() > 0.0 {
                cx.push_clip_rounded_rect(clip_rect, [radii.tl, radii.tr, radii.br, radii.bl]);
                used_rounded_clip = true;
            } else {
                cx.push_clip_rect(clip_rect);
            }
        }

        let mut child_parent_origin = dvec2(x, y);
        if needs_clip {
            if let Some(tag) = fragment.tag() {
                if let Some(scroll) = scroll_state.get(&tag.node.0) {
                    child_parent_origin.x -= scroll.x;
                    child_parent_origin.y -= scroll.y;
                }
            }
        }

        let ordered = paint_order(children);
        for idx in ordered {
            render_fragment(cx, &children[idx], child_parent_origin, content_y, clip,
                opacity, draw_bg, draw_text, draw_text_bold, draw_text_mono,
                draw_image, texture_cache, scroll_state, draw_rounded_bg,
                draw_box_shadow, draw_gradient, selection, transform_state,
                opacity_state, filter_state, draw_filter_image);
        }

        if needs_clip {
            if used_rounded_clip { cx.pop_clip_shape(); } else { cx.pop_clip_rect(); }
        }
    }
}

/// Push a Makepad clip shape for a CSS basic shape.
fn push_clip_shape(cx: &mut Cx2d, shape: &BasicShape, clip_rect: Rect, w: f32, h: f32) {
    use style::values::computed::position::Position;
    match shape {
        BasicShape::Circle(circle) => {
            let center = match &circle.position {
                GenericPositionOrAuto::Position(p) => p.clone(),
                GenericPositionOrAuto::Auto => Position::center(),
            };
            let cx_px = center.horizontal.to_used_value(Au::from_f32_px(w)).to_f32_px();
            let cy_px = center.vertical.to_used_value(Au::from_f32_px(h)).to_f32_px();
            let horizontal = compute_shape_radius_f32(cx_px, &circle.radius, 0.0, w);
            let vertical = compute_shape_radius_f32(cy_px, &circle.radius, 0.0, h);
            let r = match circle.radius {
                GenericShapeRadius::FarthestSide => horizontal.max(vertical),
                GenericShapeRadius::ClosestSide => horizontal.min(vertical),
                GenericShapeRadius::Length(_) => horizontal,
            };
            // Circle as ellipse inscribed in bounding box centered at (cx, cy).
            let ell_rect = Rect {
                pos: dvec2(clip_rect.pos.x + (cx_px - r) as f64,
                           clip_rect.pos.y + (cy_px - r) as f64),
                size: dvec2((r * 2.0) as f64, (r * 2.0) as f64),
            };
            cx.push_clip_ellipse(ell_rect);
        }
        BasicShape::Ellipse(ellipse) => {
            let center = match &ellipse.position {
                GenericPositionOrAuto::Position(p) => p.clone(),
                GenericPositionOrAuto::Auto => Position::center(),
            };
            let cx_px = center.horizontal.to_used_value(Au::from_f32_px(w)).to_f32_px();
            let cy_px = center.vertical.to_used_value(Au::from_f32_px(h)).to_f32_px();
            let rx = compute_shape_radius_f32(cx_px, &ellipse.semiaxis_x, 0.0, w);
            let ry = compute_shape_radius_f32(cy_px, &ellipse.semiaxis_y, 0.0, h);
            let ell_rect = Rect {
                pos: dvec2(clip_rect.pos.x + (cx_px - rx) as f64,
                           clip_rect.pos.y + (cy_px - ry) as f64),
                size: dvec2((rx * 2.0) as f64, (ry * 2.0) as f64),
            };
            cx.push_clip_ellipse(ell_rect);
        }
        BasicShape::Rect(rect) => {
            let bw = Au::from_f32_px(w);
            let bh = Au::from_f32_px(h);
            let top = rect.rect.0.to_used_value(bh).to_f32_px();
            let right = rect.rect.1.to_used_value(bw).to_f32_px();
            let bottom = rect.rect.2.to_used_value(bh).to_f32_px();
            let left = rect.rect.3.to_used_value(bw).to_f32_px();
            let corner_px = |c: &style::values::computed::BorderCornerRadius| {
                c.0.width.0.to_used_value(bw).to_f32_px()
                    .max(c.0.height.0.to_used_value(bh).to_f32_px())
            };
            let radii = [
                corner_px(&rect.round.top_left),
                corner_px(&rect.round.top_right),
                corner_px(&rect.round.bottom_right),
                corner_px(&rect.round.bottom_left),
            ];
            let inset_rect = Rect {
                pos: dvec2(clip_rect.pos.x + left as f64, clip_rect.pos.y + top as f64),
                size: dvec2((w - left - right).max(0.0) as f64, (h - top - bottom).max(0.0) as f64),
            };
            cx.push_clip_rounded_rect(inset_rect, radii);
        }
        _ => {
            // Polygon/path: use bounding box (no SDF for arbitrary polygons).
            cx.push_clip_rounded_rect(clip_rect, [0.0; 4]);
        }
    }
}


fn compute_shape_radius_f32(
    center: f32,
    radius: &GenericShapeRadius<style::values::computed::LengthPercentage>,
    min_edge: f32,
    max_edge: f32,
) -> f32 {
    let d_min = (min_edge - center).abs();
    let d_max = (max_edge - center).abs();
    match radius {
        GenericShapeRadius::FarthestSide => d_min.max(d_max),
        GenericShapeRadius::ClosestSide => d_min.min(d_max),
        GenericShapeRadius::Length(lp) => lp.to_used_value(Au::from_f32_px(max_edge - min_edge)).to_f32_px(),
    }
}

/// Compute the visual offset for a sticky-positioned element.
///
/// Returns (dx, dy) offset to apply to the element's rendered position.
/// For non-sticky elements, returns (0, 0).
pub(crate) fn compute_sticky_offset(
    fragment: &Fragment,
    parent_draw_origin: DVec2,
    clip: Option<(f32, f32)>,
) -> (f64, f64) {
    let bf = match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => bf,
        _ => return (0.0, 0.0),
    };

    if bf.base.style.get_box().position != ComputedPosition::Sticky {
        return (0.0, 0.0);
    }

    // Only handle vertical sticky for now (top/bottom insets).
    // The viewport bounds come from the clip parameter (viewport_top, viewport_bottom).
    let (viewport_top, viewport_bottom) = match clip {
        Some((top, bottom)) => (top as f64, bottom as f64),
        None => return (0.0, 0.0), // No scroll context, no sticky behavior.
    };

    let position = bf.base.style.get_position();
    let border_rect = bf.border_rect();
    let element_top = parent_draw_origin.y + border_rect.origin.y.to_f32_px() as f64;
    let element_h = border_rect.size.height.to_f32_px() as f64;

    let mut dy = 0.0;

    // Sticky top: if the element would scroll above viewport_top + inset, shift down.
    if let style::values::generics::position::Inset::LengthPercentage(ref lp) = position.top {
        let basis = Au::from_f32_px((viewport_bottom - viewport_top) as f32);
        let inset = lp.to_used_value(basis).to_f32_px() as f64;
        let sticky_edge = viewport_top + inset;
        if element_top < sticky_edge {
            dy = sticky_edge - element_top;
        }
    }

    // Sticky bottom: if the element would scroll below viewport_bottom - inset, shift up.
    if let style::values::generics::position::Inset::LengthPercentage(ref lp) = position.bottom {
        let basis = Au::from_f32_px((viewport_bottom - viewport_top) as f32);
        let inset = lp.to_used_value(basis).to_f32_px() as f64;
        let sticky_edge = viewport_bottom - inset;
        let element_bottom = element_top + element_h + dy;
        if element_bottom > sticky_edge {
            dy += sticky_edge - element_bottom;
        }
    }

    (0.0, dy)
}

fn rects_overlap(a: &Rect, b: &Rect) -> bool {
    a.pos.x < b.pos.x + b.size.x && a.pos.x + a.size.x > b.pos.x
        && a.pos.y < b.pos.y + b.size.y && a.pos.y + a.size.y > b.pos.y
}

fn draw_image_fragment(
    cx: &mut Cx2d, img: &ImageFragment,
    x: f64, y: f64, w: f32, h: f32,
    draw_image: &mut DrawImage, texture_cache: &mut TextureCache, opacity: f32,
) {
    let node_id = img.base.tag.map(|t| t.node.0).unwrap_or(0);
    let texture = texture_cache.entry(node_id).or_insert_with(|| {
        let data: Vec<u32> = img.pixels.chunks_exact(4).map(|px| {
            (px[2] as u32) | ((px[1] as u32) << 8) | ((px[0] as u32) << 16) | ((px[3] as u32) << 24)
        }).collect();
        let image_buffer = ImageBuffer {
            width: img.image_width as usize,
            height: img.image_height as usize,
            data,
            animation: None,
        };
        image_buffer.into_new_texture(cx.cx)
    });
    draw_image.draw_vars.set_texture(0, texture);
    draw_image.opacity = opacity;
    draw_image.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) });
}
