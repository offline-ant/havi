
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
use std::sync::LazyLock;

use base::id::WebViewId;
use layout::fragment_tree::{BoxFragment, Fragment};
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
    pub browser_scene_failure_count: u64,
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

static RENDER_STATS_ENABLED: LazyLock<bool> =
    LazyLock::new(|| matches!(std::env::var("HAVI_RENDER_STATS"), Ok(value) if value == "1"));

fn render_stats_enabled() -> bool {
    *RENDER_STATS_ENABLED
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

pub struct RenderFragmentsClippedParams<'a> {
    pub webview_id: WebViewId,
    pub cached_fragments: &'a CachedFragmentSource,
    pub host_rect: Rect,
    pub draw_bg: &'a mut DrawColor,
    pub scroll_state: &'a ScrollState,
    pub selection: Option<&'a SelectionHighlight>,
    pub frame_draw_lists: &'a mut FrameDrawListState,
    pub image_overrides: &'a havi_types::ImageOverrides,
}

pub fn render_fragments_clipped(cx: &mut Cx2d, params: RenderFragmentsClippedParams<'_>) {
    let RenderFragmentsClippedParams {
        webview_id,
        cached_fragments: _cached_fragments,
        host_rect,
        draw_bg,
        scroll_state,
        selection,
        frame_draw_lists,
        image_overrides: _image_overrides,
    } = params;

    frame_draw_lists.counters.widget_presentation_count += 1;

    let webview_origin = host_rect.pos;
    let viewport_size = host_rect.size;
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
        let scroll_changed = cache.scroll_hash != scroll_hash;
        if scroll_changed {
            update_cached_browser_document_scroll_offsets(
                &mut cache.document,
                &cache.scroll_nodes,
                scroll_state,
            );
            cache.scroll_hash = scroll_hash;
        }
        if log_render_stats {
            eprintln!(
                "[havi][render] browser_scene cache hit fragment_ptr={} scroll_changed={}",
                frag_ptr,
                scroll_changed,
            );
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
        if log_render_stats {
            eprintln!(
                "[havi][render] browser_scene cache miss fragment_ptr={} viewport=({:.1},{:.1})",
                frag_ptr,
                viewport_size.x,
                viewport_size.y,
            );
        }
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
                frame_draw_lists.counters.browser_scene_failure_count += 1;
                frame_draw_lists.browser_document_cache = None;
                eprintln!("[havi][render] browser_scene cached draw failed: {err:?}");
            }
        }
    }

    if frame_draw_lists.browser_renderer.is_none() {
        frame_draw_lists.browser_renderer = Some(MpBrowserRenderer::new(cx.cx));
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
        frame_draw_lists
            .browser_renderer
            .as_mut()
            .unwrap()
            .resource_registry_mut(),
        previous_document,
    ) {
        Ok(browser_document) => browser_document,
        Err(err) => {
            frame_draw_lists.counters.browser_scene_failure_count += 1;
            eprintln!("[havi][render] browser_scene builder failed: {err}");
            return;
        }
    };

    frame_draw_lists.counters.scene_rebuild_count += 1;
    frame_draw_lists.counters.scene_submit_count += 1;
    frame_draw_lists.counters.browser_scene_present_count += 1;
    let document_rect = Rect {
        pos: webview_origin,
        size: viewport_size,
    };
    match frame_draw_lists
        .browser_renderer
        .as_mut()
        .unwrap()
        .draw_document(cx, &browser_document.document, document_rect)
    {
        Ok(stats) => {
            if log_render_stats {
                log_browser_scene_stats(&stats);
            }
            let browser_scene_builder::BuiltBrowserDocument {
                document,
                scroll_nodes,
            } = browser_document;
            frame_draw_lists.browser_document_cache = Some(BrowserDocumentCacheEntry {
                fragment_ptr: frag_ptr,
                viewport_size,
                scroll_hash,
                document,
                scroll_nodes,
            });
            paint_selection_overlay(cx, draw_bg, selection);
        }
        Err(err) => {
            frame_draw_lists.counters.browser_scene_failure_count += 1;
            frame_draw_lists.browser_document_cache = None;
            eprintln!("[havi][render] browser_scene draw failed: {err:?}");
        }
    }
}

pub fn is_scroll_container(bf: &BoxFragment) -> bool {
    let style = bf.style();
    let ov = style.get_box();
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
                let cbf = cbf.borrow();
                let br = cbf.border_rect();
                (
                    br.origin.x.to_f32_px() as f64 + br.size.width.to_f32_px() as f64,
                    br.origin.y.to_f32_px() as f64 + br.size.height.to_f32_px() as f64,
                )
            }
            Fragment::AbsoluteOrFixedPositioned(_) => (0.0, 0.0),
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
