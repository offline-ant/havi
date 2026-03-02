//! Render layout fragments using Makepad's native draw pipeline.
//!
//! Builds a stacking context tree from fragments (CSS 2.1 Appendix E) and walks
//! it in correct paint order, emitting Makepad draw calls.

mod background;
mod hit_test;
mod makepad_builder;
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

use havi_types::Fragment;
use makepad_widgets::*;
use makepad_widgets::makepad_draw::Texture;
use makepad_widgets::makepad_draw::draw_list_2d::DrawList2d;
use app_units::Au;
use style::computed_values::position::T as ComputedPosition;

pub use shaders::{DrawRoundedColor, DrawBoxShadow, DrawGradient, DrawFilterImage};
pub use hit_test::{hit_test, find_scroll_container};
pub use stacking_context::CachedStackingContextTree;

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
    let sc = stacking_context::build_stacking_context_tree(fragments);
    let mut state = makepad_builder::MakepadDrawState {
        draw_bg, draw_text, draw_text_bold, draw_text_mono,
        draw_image, texture_cache, scroll_state, draw_rounded_bg,
        draw_box_shadow, draw_gradient, selection, transform_state,
        opacity_state, filter_state, draw_filter_image,
    };
    makepad_builder::paint_stacking_context(cx, &sc, origin, None, 1.0, &mut state);
}

/// Draw fragments with viewport clipping, using a pre-built stacking context tree.
pub fn render_fragments_clipped(
    cx: &mut Cx2d,
    cached_tree: &CachedStackingContextTree,
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
    let mut state = makepad_builder::MakepadDrawState {
        draw_bg, draw_text, draw_text_bold, draw_text_mono,
        draw_image, texture_cache, scroll_state, draw_rounded_bg,
        draw_box_shadow, draw_gradient, selection, transform_state,
        opacity_state, filter_state, draw_filter_image,
    };
    makepad_builder::paint_stacking_context(
        cx, cached_tree.tree(), origin, Some((viewport_top, viewport_bottom)), 1.0, &mut state,
    );
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

    let (viewport_top, viewport_bottom) = match clip {
        Some((top, bottom)) => (top as f64, bottom as f64),
        None => return (0.0, 0.0),
    };

    let position = bf.base.style.get_position();
    let border_rect = bf.border_rect();
    let element_top = parent_draw_origin.y + border_rect.origin.y.to_f32_px() as f64;
    let element_h = border_rect.size.height.to_f32_px() as f64;

    let mut dy = 0.0;

    if let style::values::generics::position::Inset::LengthPercentage(ref lp) = position.top {
        let basis = Au::from_f32_px((viewport_bottom - viewport_top) as f32);
        let inset = lp.to_used_value(basis).to_f32_px() as f64;
        let sticky_edge = viewport_top + inset;
        if element_top < sticky_edge {
            dy = sticky_edge - element_top;
        }
    }

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
