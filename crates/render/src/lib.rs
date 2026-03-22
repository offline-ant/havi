//! Render layout fragments using a Servo-shaped semantic scene with a Makepad backend.
//!
//! Architecture boundary:
//! - layout publishes the shared semantic fragment model through shared state
//! - render lowers that semantic fragment tree into stacking contexts and `RenderScene`
//! - hit testing, clip evaluation, semantic planning, and Makepad lowering consume `RenderScene`
//! - Makepad modules execute the already-built scene and do not reconstruct layout semantics
//!
//! Source-of-truth split:
//! - semantic lowering and scene construction: `layout_adapter`, `layout_stacking_context`,
//!   `frame_builder`, `scene`, `scene_builder`, `hit_test`
//! - Makepad boundary lowering: `mp_scene_lowering`, `makepad_builder`
//! - backend-specific transform fallback: `transform`

mod background;
mod frame_builder;
mod fragment_source;
mod hit_test;
mod layout_adapter;
mod makepad_builder;
mod makepad_fragments;
mod mp_scene_lowering;
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
use makepad_compositor::{MpRenderer, MpSurface};
use makepad_widgets::*;
use makepad_widgets::makepad_draw::draw_list_2d::DrawList2d;
use makepad_widgets::makepad_draw::Texture;
use style::computed_values::overflow_x::T as ComputedOverflow;

pub use shaders::{
    DrawBoxShadow, DrawGradient, DrawRoundedColor, DrawVideoYuv,
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SceneSurfaceKey {
    pub paint_container_id: usize,
    pub run_index: usize,
}

pub(crate) struct SceneSurfaceCacheEntry {
    pub surface: MpSurface,
    pub draw_list: DrawList2d,
}

#[derive(Default)]
pub struct FrameDrawListState {
    pub(crate) renderer: Option<MpRenderer>,
    pub(crate) surfaces: HashMap<SceneSurfaceKey, SceneSurfaceCacheEntry>,
}

impl FrameDrawListState {
    pub fn clear(&mut self) {
        self.surfaces.clear();
    }
}

/// Draw fragments with viewport clipping, using a pre-built semantic scene.
///
/// Scene construction stays in page space. The compositor owns all placement
/// via `host_rect` and `page_to_host` on the scene root.
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
    image_overrides: &havi_types::ImageOverrides,
) {
    let widget_rect = cx.turtle().rect();
    let webview_origin = widget_rect.pos;
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
        viewport_size,
    );
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
        frame_draw_lists,
        image_overrides,
    };
    makepad_builder::paint_scene(
        cx,
        &scene,
        webview_origin,
        viewport_size,
        &mut state,
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
