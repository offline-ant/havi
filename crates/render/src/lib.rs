
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
use havi_types::fragment_tree as published;
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
    pub document_rebuild_count: u64,
    pub scene_rebuild_count: u64,
    pub scene_submit_count: u64,
    pub widget_presentation_count: u64,
    pub browser_scene_present_count: u64,
    pub browser_scene_failure_count: u64,
    pub document_cache_hit_count: u64,
    pub document_cache_miss_count: u64,
    pub lowered_scene_cache_hit_count: u64,
    pub lowered_scene_cache_miss_count: u64,
    pub retained_document_scroll_patch_count: u64,
    pub retained_scene_scroll_relower_count: u64,
    pub prepared_text_batch_hit_count: u64,
    pub prepared_text_batch_miss_count: u64,
    pub prepared_text_batch_rebuild_count: u64,
    pub glyph_residency_hit_count: u64,
    pub glyph_residency_miss_count: u64,
    pub glyph_cache_reset_count: u64,
    pub atlas_page_alloc_count: u64,
    pub msdf_request_queue_count: u64,
    pub msdf_completion_count: u64,
    pub synchronous_fallback_glyph_generation_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RetainedBrowserSceneStructuralKey {
    fragment_ptr: usize,
    viewport_size: DVec2,
    resource_generation: u64,
}

#[derive(Clone)]
struct BrowserDocumentCacheEntry {
    structural_key: RetainedBrowserSceneStructuralKey,
    scroll_hash: u64,
    document: makepad_browser_scene::MpDocument,
    scroll_nodes: browser_scene_builder::BrowserDocumentScrollNodes,
    lowered_scene: Option<makepad_browser_scene::MpRetainedBrowserScene>,
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

fn accumulate_frame_counters(counters: &mut RenderPathCounters, stats: &makepad_browser_scene::MpRendererStats) {
    counters.prepared_text_batch_hit_count += stats.prepared_text_batch_hit_count as u64;
    counters.prepared_text_batch_miss_count += stats.prepared_text_batch_miss_count as u64;
    counters.prepared_text_batch_rebuild_count += stats.prepared_text_batch_rebuild_count as u64;
    counters.glyph_residency_hit_count += stats.glyph_residency_hit_count as u64;
    counters.glyph_residency_miss_count += stats.glyph_residency_miss_count as u64;
    counters.glyph_cache_reset_count += stats.glyph_cache_reset_count as u64;
    counters.atlas_page_alloc_count += stats.atlas_page_alloc_count as u64;
    counters.msdf_request_queue_count += stats.msdf_request_queue_count as u64;
    counters.msdf_completion_count += stats.msdf_completion_count as u64;
    counters.synchronous_fallback_glyph_generation_count +=
        stats.synchronous_fallback_glyph_generation_count as u64;
}

static RENDER_STATS_ENABLED: LazyLock<bool> =
    LazyLock::new(|| matches!(std::env::var("HAVI_RENDER_STATS"), Ok(value) if value == "1"));

fn render_stats_enabled() -> bool {
    *RENDER_STATS_ENABLED
}

fn log_browser_scene_stats(stats: &makepad_browser_scene::MpRendererStats) {
    eprintln!(
        "[havi][render] browser_scene stats direct_primitives={} isolated_boundaries={} isolated_primitives={} compositor_surfaces={} offscreen_pixel_area={} scratch_surfaces={} scratch_reused={} scratch_new={} prepared_hits={} prepared_misses={} prepared_rebuilds={} glyph_hits={} glyph_misses={} glyph_resets={} atlas_pages={} msdf_queued={} msdf_completed={} sync_fallback_glyphs={}",
        stats.direct_primitive_count,
        stats.isolated_boundary_count,
        stats.isolated_primitive_count,
        stats.compositor_surface_count,
        stats.total_offscreen_pixel_area,
        stats.scratch_surface_count,
        stats.scratch_surface_reuse_count,
        stats.scratch_surface_new_alloc_count,
        stats.prepared_text_batch_hit_count,
        stats.prepared_text_batch_miss_count,
        stats.prepared_text_batch_rebuild_count,
        stats.glyph_residency_hit_count,
        stats.glyph_residency_miss_count,
        stats.glyph_cache_reset_count,
        stats.atlas_page_alloc_count,
        stats.msdf_request_queue_count,
        stats.msdf_completion_count,
        stats.synchronous_fallback_glyph_generation_count,
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

    let viewport_size = host_rect.size;
    let document_rect = Rect {
        pos: host_rect.pos,
        size: viewport_size,
    };
    let layout_source = layout_adapter::LayoutFragmentSource::new(webview_id);
    let Some(fragments) = layout_source.fragments_arc() else {
        return;
    };
    let frag_ptr = std::sync::Arc::as_ptr(&fragments) as usize;
    let scroll_hash = hash_scroll_state(scroll_state);
    let log_render_stats = render_stats_enabled();

    if frame_draw_lists.browser_renderer.is_none() {
        frame_draw_lists.browser_renderer = Some(MpBrowserRenderer::new(cx.cx));
    }

    let renderer_resource_generation = frame_draw_lists
        .browser_renderer
        .as_ref()
        .unwrap()
        .resource_generation();
    let structural_key = RetainedBrowserSceneStructuralKey {
        fragment_ptr: frag_ptr,
        viewport_size,
        resource_generation: renderer_resource_generation,
    };

    let document_cache_hit = frame_draw_lists
        .browser_document_cache
        .as_ref()
        .map(|cache| {
            cache.structural_key.fragment_ptr == frag_ptr
                && cache.structural_key.viewport_size == viewport_size
        })
        .unwrap_or(false);
    if document_cache_hit {
        frame_draw_lists.counters.document_cache_hit_count += 1;
    } else {
        frame_draw_lists.counters.document_cache_miss_count += 1;
    }

    if document_cache_hit {
        let (scroll_changed, resource_changed) = {
            let cache = frame_draw_lists.browser_document_cache.as_mut().unwrap();
            let scroll_changed = cache.scroll_hash != scroll_hash;
            if scroll_changed {
                update_cached_browser_document_scroll_offsets(
                    &mut cache.document,
                    &cache.scroll_nodes,
                    scroll_state,
                );
                cache.scroll_hash = scroll_hash;
                frame_draw_lists.counters.retained_document_scroll_patch_count += 1;
                if cache.lowered_scene.is_some() {
                    frame_draw_lists.counters.retained_scene_scroll_relower_count += 1;
                }
                // Temporary until retained scroll patching lands: keep scroll out
                // of the structural key, update the retained document in place,
                // then drop only the lowered scene.
                cache.lowered_scene = None;
            }
            let resource_changed =
                cache.structural_key.resource_generation != renderer_resource_generation;
            if resource_changed {
                cache.lowered_scene = None;
            }
            (scroll_changed, resource_changed)
        };

        let lowered_scene_hit = frame_draw_lists
            .browser_document_cache
            .as_ref()
            .and_then(|cache| cache.lowered_scene.as_ref())
            .map(|scene| scene.resource_generation() == renderer_resource_generation)
            .unwrap_or(false);

        if lowered_scene_hit {
            frame_draw_lists.counters.lowered_scene_cache_hit_count += 1;
            frame_draw_lists.counters.scene_submit_count += 1;
            frame_draw_lists.counters.browser_scene_present_count += 1;
            let renderer = frame_draw_lists.browser_renderer.as_mut().unwrap();
            let cache = frame_draw_lists.browser_document_cache.as_mut().unwrap();
            let retained_scene = cache.lowered_scene.as_mut().unwrap();
            renderer.patch_retained_scene_host_rect(retained_scene, document_rect);
            let stats = renderer.draw_retained_scene(cx, retained_scene);
            accumulate_frame_counters(&mut frame_draw_lists.counters, &stats);
            if log_render_stats {
                eprintln!(
                    "[havi][render] browser_document hit fragment_ptr={} lowered_scene=hit scroll_changed={} resource_changed={}",
                    frag_ptr,
                    scroll_changed,
                    resource_changed,
                );
                log_browser_scene_stats(&stats);
            }
            paint_selection_overlay(cx, draw_bg, selection);
            return;
        }

        frame_draw_lists.counters.lowered_scene_cache_miss_count += 1;
        frame_draw_lists.counters.scene_rebuild_count += 1;
        let renderer = frame_draw_lists.browser_renderer.as_mut().unwrap();
        let cache = frame_draw_lists.browser_document_cache.as_mut().unwrap();
        match renderer.lower_retained_document(&cache.document, document_rect) {
            Ok(mut retained_scene) => {
                renderer.patch_retained_scene_host_rect(&mut retained_scene, document_rect);
                cache.structural_key = RetainedBrowserSceneStructuralKey {
                    resource_generation: retained_scene.resource_generation(),
                    ..structural_key
                };
                cache.lowered_scene = Some(retained_scene);
                frame_draw_lists.counters.scene_submit_count += 1;
                frame_draw_lists.counters.browser_scene_present_count += 1;
                let stats = renderer.draw_retained_scene(cx, cache.lowered_scene.as_mut().unwrap());
                accumulate_frame_counters(&mut frame_draw_lists.counters, &stats);
                if log_render_stats {
                    eprintln!(
                        "[havi][render] browser_document hit fragment_ptr={} lowered_scene=miss scroll_changed={} resource_changed={}",
                        frag_ptr,
                        scroll_changed,
                        resource_changed,
                    );
                    log_browser_scene_stats(&stats);
                }
                paint_selection_overlay(cx, draw_bg, selection);
                return;
            }
            Err(err) => {
                frame_draw_lists.counters.browser_scene_failure_count += 1;
                frame_draw_lists.browser_document_cache = None;
                eprintln!("[havi][render] retained browser_scene lower failed: {err:?}");
                return;
            }
        }
    }

    if log_render_stats {
        eprintln!(
            "[havi][render] browser_document miss fragment_ptr={} viewport=({:.1},{:.1})",
            frag_ptr,
            viewport_size.x,
            viewport_size.y,
        );
    }

    let previous_document = frame_draw_lists
        .browser_document_cache
        .as_ref()
        .filter(|cache| cache.structural_key.fragment_ptr == frag_ptr)
        .map(|cache| &cache.document);
    let browser_document = match browser_scene_builder::try_build_browser_document(
        cx,
        fragments.as_ref(),
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

    frame_draw_lists.counters.document_rebuild_count += 1;
    frame_draw_lists.counters.scene_rebuild_count += 1;
    frame_draw_lists.counters.lowered_scene_cache_miss_count += 1;
    let renderer = frame_draw_lists.browser_renderer.as_mut().unwrap();
    let browser_scene_builder::BuiltBrowserDocument {
        document,
        scroll_nodes,
    } = browser_document;
    match renderer.lower_retained_document(&document, document_rect) {
        Ok(mut retained_scene) => {
            renderer.patch_retained_scene_host_rect(&mut retained_scene, document_rect);
            frame_draw_lists.counters.scene_submit_count += 1;
            frame_draw_lists.counters.browser_scene_present_count += 1;
            let stats = renderer.draw_retained_scene(cx, &mut retained_scene);
            accumulate_frame_counters(&mut frame_draw_lists.counters, &stats);
            if log_render_stats {
                log_browser_scene_stats(&stats);
            }
            frame_draw_lists.browser_document_cache = Some(BrowserDocumentCacheEntry {
                structural_key: RetainedBrowserSceneStructuralKey {
                    resource_generation: retained_scene.resource_generation(),
                    ..structural_key
                },
                scroll_hash,
                document,
                scroll_nodes,
                lowered_scene: Some(retained_scene),
            });
            paint_selection_overlay(cx, draw_bg, selection);
        }
        Err(err) => {
            frame_draw_lists.counters.browser_scene_failure_count += 1;
            frame_draw_lists.browser_document_cache = None;
            eprintln!("[havi][render] retained browser_scene draw failed: {err:?}");
        }
    }
}

pub fn is_scroll_container(bf: &published::BoxFragment) -> bool {
    let style = bf.style();
    let ov = style.get_box();
    matches!(ov.overflow_x, ComputedOverflow::Auto | ComputedOverflow::Scroll)
        || matches!(ov.overflow_y, ComputedOverflow::Auto | ComputedOverflow::Scroll)
}
