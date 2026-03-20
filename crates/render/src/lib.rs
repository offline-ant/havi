//! Render layout fragments using a Servo-shaped semantic scene with a Makepad backend.
//!
//! Architecture boundary:
//! - layout publishes the shared semantic fragment model through shared state
//! - render lowers that semantic fragment tree into stacking contexts and `RenderScene`
//! - hit testing, clip evaluation, render planning, and compositor planning consume `RenderScene`
//! - Makepad modules execute the already-built scene and do not reconstruct layout semantics
//!
//! Source-of-truth split:
//! - semantic lowering and scene construction: `layout_adapter`, `layout_stacking_context`,
//!   `frame_builder`, `scene`, `scene_builder`, `hit_test`
//! - scene-native storage now lives in `scene` and `scene_builder`
//! - active scene identity is split into spatial nodes and paint containers
//! - builder APIs now expose spatial-node creation and paint-container attachment separately
//! - leftover compatibility modules pending deletion: `frame_tree`, `clip_tree`
//! - backend execution: `makepad_builder`, `makepad_fragments`, `makepad_effects`,
//!   `render_plan`, `compositor_scene`
//! - backend-specific transform fallback: `transform`

mod background;
mod clip_tree;
mod compositor_scene;
mod frame_builder;
mod frame_tree;
mod fragment_source;
mod hit_test;
mod layout_adapter;
mod makepad_builder;
mod makepad_clip;
mod makepad_effects;
mod makepad_fragments;
mod paint_items;
mod reference_frame;
mod render_plan;
mod scene;
mod scene_builder;
pub mod shaders;
pub mod video_texture_map;
pub(crate) mod layout_stacking_context;
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

use base::id::WebViewId;
use havi_fragment_semantics::fragment_tree::BoxFragment;
use havi_fragment_semantics::Fragment;
use makepad_widgets::*;
use makepad_widgets::makepad_draw::Texture;
use makepad_widgets::makepad_draw::draw_list_2d::DrawList2d;
use style::computed_values::overflow_x::T as ComputedOverflow;

pub use shaders::{
    DrawBoxShadow, DrawFilterImage, DrawGradient, DrawRoundedColor, DrawVideoYuv,
};
pub use fragment_source::CachedFragmentSource;

/// Cache for image textures, keyed by OpaqueNode id.
/// Each entry tracks the texture and the byte-range hash used to create it,
/// so we can skip re-uploading unchanged frames.
pub type TextureCache = HashMap<usize, TextureCacheEntry>;

/// A cached texture with metadata for change detection.
pub struct TextureCacheEntry {
    pub texture: Texture,
    /// Hash of the byte range used to create this texture.
    /// Used to skip re-uploading unchanged pixel data.
    data_hash: u64,
}

/// Pre-computed selection highlight rectangles in root visual coordinates.
#[derive(Clone, Debug)]
pub struct SelectionHighlight {
    pub color: Vec4f,
    pub rects: Vec<Rect>,
}

/// Per-element scroll offsets for overflow containers, keyed by OpaqueNode id.
pub type ScrollState = HashMap<usize, DVec2>;

pub struct FrameDrawList {
    pub draw_list: DrawList2d,
}

pub type FrameDrawListState = HashMap<crate::frame_tree::FrameKey, FrameDrawList>;

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

/// Draw fragments with viewport clipping, using a pre-built stacking context tree.
///
/// Fragment coordinates are page-relative (starting at 0,0). `cx.turtle().rect()`
/// reports the current widget rect in the active Makepad pass, and that pass is
/// not always the same kind of surface. During ordinary `CachedView`
/// texture-caching the webview draws into its own cache texture, so
/// `widget_rect.pos` is `(0,0)` in that local texture space. During framebuffer
/// readback paths such as `webview.take_screenshot`, Makepad renders the widget
/// into the window framebuffer and `widget_rect.pos` is the webview's actual
/// window-space position. The scene origin therefore has to include
/// `widget_rect.pos` so fragment draw positions line up in both passes; only the
/// viewport scroll offset is always subtracted from y.
pub fn render_fragments_clipped(
    cx: &mut Cx2d,
    webview_id: WebViewId,
    _cached_fragments: &CachedFragmentSource,
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
    draw_video_yuv: &mut DrawVideoYuv,
    selection: Option<&SelectionHighlight>,
    frame_draw_lists: &mut FrameDrawListState,
    opacity_state: &mut OpacityState,
    filter_state: &mut FilterState,
    draw_filter_image: &mut DrawFilterImage,
    image_overrides: &havi_types::ImageOverrides,
) {
    let widget_rect = cx.turtle().rect();
    // Fragment coordinates are page-relative. Keep the widget's absolute pass
    // position in the scene origin so cached texture passes and direct window
    // passes both line up with Makepad's clip space.
    let scroll_origin = dvec2(widget_rect.pos.x, widget_rect.pos.y - viewport_top as f64);
    let viewport_size = dvec2(
        widget_rect.size.x,
        (viewport_bottom - viewport_top) as f64,
    );
    let layout_source = layout_adapter::LayoutFragmentSource::new(webview_id);
    let Some(fragments) = layout_source.fragments_arc() else {
        return;
    };
    let scene = frame_builder::build_scene(
        &fragments,
        scroll_state,
        scroll_origin,
        viewport_size,
    );
    frame_draw_lists.clear();
    let mut state = makepad_builder::MakepadDrawState {
        draw_bg,
        draw_text,
        draw_text_bold,
        draw_text_mono,
        draw_image,
        texture_cache,
        draw_rounded_bg,
        draw_box_shadow,
        draw_gradient,
        draw_video_yuv,
        selection,
        opacity_state,
        filter_state,
        draw_filter_image,
        frame_draw_lists,
        image_overrides,
    };
    makepad_builder::paint_scene(
        cx,
        &scene,
        viewport_size,
        &mut state,
        1.0,
    );
}

/// Compute the visual offset for a sticky-positioned element.
///
/// Returns (dx, dy) offset to apply to the element's rendered position.
/// For non-sticky elements, returns (0, 0).
/// Check if a box fragment establishes a scroll container (overflow != visible).
pub fn is_scroll_container(bf: &BoxFragment) -> bool {
    let ov = bf.base.style.get_box();
    matches!(ov.overflow_x, ComputedOverflow::Auto | ComputedOverflow::Scroll)
        || matches!(ov.overflow_y, ComputedOverflow::Auto | ComputedOverflow::Scroll)
}

/// Compute the scrollable content bounds for a box fragment.
///
/// Returns (max_scroll_x, max_scroll_y) — the maximum scroll offsets.
/// The scroll port is the padding box (per CSS spec). The scrollable extent
/// is the maximum of children's border-rect extents (in content-rect coords).
/// Max scroll = content_extent - content_rect_size (since children are in
/// content-rect coordinates, the visible area in that coordinate system is
/// the content rect size).
pub fn scroll_bounds(bf: &BoxFragment) -> (f64, f64) {
    let content_w = bf.content_rect().size.width.to_f32_px() as f64;
    let content_h = bf.content_rect().size.height.to_f32_px() as f64;

    // Compute content bounds as the union of all children's border rects
    // (children are positioned in content-rect coordinates).
    let mut max_x: f64 = 0.0;
    let mut max_y: f64 = 0.0;
    for child in &bf.children {
        let (right, bottom) = match child {
            Fragment::Box(cbf) | Fragment::Float(cbf) => {
                let br = cbf.border_rect();
                (
                    br.origin.x.to_f32_px() as f64 + br.size.width.to_f32_px() as f64,
                    br.origin.y.to_f32_px() as f64 + br.size.height.to_f32_px() as f64,
                )
            }
            Fragment::AbsoluteOrFixedPositioned { .. } => {
                (0.0, 0.0)
            }
            _ => {
                let cr = child.content_rect();
                (
                    cr.origin.x.to_f32_px() as f64 + cr.size.width.to_f32_px() as f64,
                    cr.origin.y.to_f32_px() as f64 + cr.size.height.to_f32_px() as f64,
                )
            }
        };
        max_x = max_x.max(right);
        max_y = max_y.max(bottom);
    }

    let scroll_max_x = (max_x - content_w).max(0.0);
    let scroll_max_y = (max_y - content_h).max(0.0);
    (scroll_max_x, scroll_max_y)
}
