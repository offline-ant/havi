
mod background;
mod browser_scene_builder;
mod browser_scene_primitives;
mod fragment_source;
mod layout_adapter;
mod paint_items;
mod reference_frame;
mod render_plan;
pub mod shaders;
pub mod video_texture_map;
pub(crate) mod layout_stacking_context;
mod transform;

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
use makepad_browser_scene::MpBrowserRenderer;
use makepad_widgets::*;
use style::computed_values::overflow_x::T as ComputedOverflow;

pub use fragment_source::CachedFragmentSource;
pub use shaders::{
    DrawBoxShadow, DrawGradient, DrawRoundedColor, DrawVideoYuv,
};

#[derive(Clone, Debug)]
pub struct SelectionHighlight {
    pub color: Vec4f,
    pub rects: Vec<Rect>,
}

pub type ScrollState = HashMap<usize, DVec2>;

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderPathCounters {
    pub scene_rebuild_count: u64,
    pub scene_submit_count: u64,
    pub widget_presentation_count: u64,
    pub browser_scene_present_count: u64,
    pub browser_scene_fallback_count: u64,
}

#[derive(Clone)]
struct BrowserDocumentCacheEntry {
    fragment_ptr: usize,
    viewport_size: DVec2,
    scroll_hash: u64,
    document: makepad_browser_scene::MpDocument,
    scroll_nodes: browser_scene_builder::BrowserDocumentScrollNodes,
}

#[derive(Default)]
pub struct FrameDrawListState {
    pub(crate) browser_renderer: Option<MpBrowserRenderer>,
    pub(crate) browser_document_cache: Option<BrowserDocumentCacheEntry>,
    pub counters: RenderPathCounters,
}

impl FrameDrawListState {
    pub fn clear(&mut self) {
        self.browser_document_cache = None;
    }
}

fn hash_scroll_state(scroll_state: &ScrollState) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut entries: Vec<_> = scroll_state.iter().collect();
    entries.sort_by_key(|(id, _)| *id);
    let mut hasher = DefaultHasher::new();
    for (id, offset) in entries {
        id.hash(&mut hasher);
        offset.x.to_bits().hash(&mut hasher);
        offset.y.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn update_cached_browser_document_scroll_offsets(
    document: &mut makepad_browser_scene::MpDocument,
    scroll_nodes: &browser_scene_builder::BrowserDocumentScrollNodes,
    scroll_state: &ScrollState,
) -> usize {
    let mut updated = document.scene.update_scroll_offsets(
        scroll_nodes.spatial_nodes.iter().map(|(node_id, spatial_id)| {
            (
                *spatial_id,
                scroll_state
                    .get(node_id)
                    .copied()
                    .unwrap_or_else(|| dvec2(0.0, 0.0)),
            )
        }),
    );
    for child_document in &mut document.child_documents {
        if let Some(child_scroll_nodes) = scroll_nodes.child_documents.get(&child_document.pipeline_id) {
            updated += update_cached_browser_document_scroll_offsets(
                child_document.document.as_mut(),
                child_scroll_nodes,
                scroll_state,
            );
        }
    }
    updated
}

fn render_stats_enabled() -> bool {
    matches!(std::env::var("HAVI_RENDER_STATS"), Ok(value) if value == "1")
}

fn log_browser_scene_stats(stats: &makepad_browser_scene::MpRendererStats) {
    eprintln!(
        "[havi][render] browser_scene stats direct_primitives={} isolated_boundaries={} isolated_primitives={} compositor_surfaces={} offscreen_pixel_area={} scratch_surfaces={} scratch_reused={} scratch_new={}",
        stats.direct_primitive_count,
        stats.isolated_boundary_count,
        stats.isolated_primitive_count,
        stats.compositor_surface_count,
        stats.total_offscreen_pixel_area,
        stats.scratch_surface_count,
        stats.scratch_surface_reuse_count,
        stats.scratch_surface_new_alloc_count,
    );
}

pub fn browser_scene_script_mod(vm: &mut ScriptVm) -> ScriptValue {
    makepad_browser_scene::script_mod(vm)
}

fn paint_selection_overlay(cx: &mut Cx2d, draw_bg: &mut DrawColor, selection: Option<&SelectionHighlight>) {
    let Some(selection) = selection else {
        return;
    };
    draw_bg.color = selection.color;
    for rect in &selection.rects {
        if rect.size.x > 0.0 && rect.size.y > 0.0 {
            draw_bg.draw_abs(cx, *rect);
        }
    }
}

pub fn render_fragments_clipped(
    cx: &mut Cx2d,
    webview_id: WebViewId,
    _cached_fragments: &CachedFragmentSource,
    viewport_top: f32,
    viewport_bottom: f32,
    draw_bg: &mut DrawColor,
    scroll_state: &ScrollState,
    selection: Option<&SelectionHighlight>,
    frame_draw_lists: &mut FrameDrawListState,
    _image_overrides: &havi_types::ImageOverrides,
) {
    frame_draw_lists.counters.widget_presentation_count += 1;

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
    let frag_ptr = std::sync::Arc::as_ptr(&fragments) as usize;
    let scroll_hash = hash_scroll_state(scroll_state);
    let log_render_stats = render_stats_enabled();

    let cached_draw = if let Some(cache) = frame_draw_lists.browser_document_cache.as_mut().filter(|cache| {
        cache.fragment_ptr == frag_ptr && cache.viewport_size == viewport_size
    }) {
        if cache.scroll_hash != scroll_hash {
            update_cached_browser_document_scroll_offsets(
                &mut cache.document,
                &cache.scroll_nodes,
                scroll_state,
            );
            cache.scroll_hash = scroll_hash;
        }
        if frame_draw_lists.browser_renderer.is_none() {
            frame_draw_lists.browser_renderer = Some(MpBrowserRenderer::new(cx.cx));
        }
        frame_draw_lists.counters.scene_submit_count += 1;
        frame_draw_lists.counters.browser_scene_present_count += 1;
        Some(
            frame_draw_lists
                .browser_renderer
                .as_mut()
                .unwrap()
                .draw_document(
                    cx,
                    &cache.document,
                    Rect {
                        pos: webview_origin,
                        size: viewport_size,
                    },
                ),
        )
    } else {
        None
    };
    if let Some(cached_draw) = cached_draw {
        match cached_draw {
            Ok(stats) => {
                if log_render_stats {
                    log_browser_scene_stats(&stats);
                }
                paint_selection_overlay(cx, draw_bg, selection);
                return;
            }
            Err(err) => {
                frame_draw_lists.counters.browser_scene_fallback_count += 1;
                frame_draw_lists.browser_document_cache = None;
                eprintln!("[havi][render] browser_scene cached draw failed: {err:?}");
            }
        }
    }

    let previous_document = frame_draw_lists
        .browser_document_cache
        .as_ref()
        .filter(|cache| cache.fragment_ptr == frag_ptr)
        .map(|cache| &cache.document);
    let browser_document = match browser_scene_builder::try_build_browser_document(
        cx,
        &fragments,
        scroll_state,
        viewport_size,
        previous_document,
    ) {
        Ok(browser_document) => browser_document,
        Err(err) => {
            frame_draw_lists.counters.browser_scene_fallback_count += 1;
            eprintln!("[havi][render] browser_scene builder failed: {err}");
            return;
        }
    };

    if frame_draw_lists.browser_renderer.is_none() {
        frame_draw_lists.browser_renderer = Some(MpBrowserRenderer::new(cx.cx));
    }
    frame_draw_lists.counters.scene_rebuild_count += 1;
    frame_draw_lists.counters.scene_submit_count += 1;
    frame_draw_lists.counters.browser_scene_present_count += 1;
    match frame_draw_lists
        .browser_renderer
        .as_mut()
        .unwrap()
        .draw_document(
            cx,
            &browser_document.document,
            Rect {
                pos: webview_origin,
                size: viewport_size,
            },
        ) {
        Ok(stats) => {
            if log_render_stats {
                log_browser_scene_stats(&stats);
            }
            frame_draw_lists.browser_document_cache = Some(BrowserDocumentCacheEntry {
                fragment_ptr: frag_ptr,
                viewport_size,
                scroll_hash,
                document: browser_document.document.clone(),
                scroll_nodes: browser_document.scroll_nodes,
            });
            paint_selection_overlay(cx, draw_bg, selection);
        }
        Err(err) => {
            frame_draw_lists.counters.browser_scene_fallback_count += 1;
            frame_draw_lists.browser_document_cache = None;
            eprintln!("[havi][render] browser_scene draw failed: {err:?}");
        }
    }
}

pub fn is_scroll_container(bf: &BoxFragment) -> bool {
    let ov = bf.base.style.get_box();
    matches!(ov.overflow_x, ComputedOverflow::Auto | ComputedOverflow::Scroll)
        || matches!(ov.overflow_y, ComputedOverflow::Auto | ComputedOverflow::Scroll)
}

// Child fragment coordinates are measured in content-box space, so max scroll
// is content extent minus visible content size.
pub fn scroll_bounds(bf: &BoxFragment) -> (f64, f64) {
    let content_w = bf.content_rect().size.width.to_f32_px() as f64;
    let content_h = bf.content_rect().size.height.to_f32_px() as f64;

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
