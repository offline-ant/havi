use crate::browser_scroll::{BrowserScrollCommit, BrowserScrollController};
use makepad_widgets::draw_list_2d::{DrawList2d, DrawListExt};
use makepad_widgets::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;


// ---------------------------------------------------------------------------
// Widget registration
// ---------------------------------------------------------------------------

script_mod! {
    use mod.prelude.widgets.*
    use mod.draw

    mod.widgets.DrawCachedSurface = mod.std.set_type_default() do #(DrawCachedSurface::script_shader(vm)) {
        ..draw.DrawQuad
        image: texture_2d(float)

        pixel: fn() {
            return self.image.sample(self.pos)
        }
    }

    mod.widgets.ServoWebViewBase = #(ServoWebView::register_widget(vm))
    mod.widgets.ServoWebView = set_type_default() do mod.widgets.ServoWebViewBase{
        width: Fill
        height: Fill
    }
}

#[derive(Default)]
struct FrameDrawLists(havi_render::FrameDrawListState);

#[derive(Script, ScriptHook, Debug)]
#[repr(C)]
struct DrawCachedSurface {
    #[deref]
    draw_super: DrawQuad,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct BrowserSurfaceCacheKey {
    fragment_ptr: usize,
    viewport_width_bits: u64,
    viewport_height_bits: u64,
    dpi_bits: u64,
    scroll_hash: u64,
    selection_hash: u64,
}

struct BrowserSurfaceCache {
    pass: DrawPass,
    draw_list: DrawList2d,
    color_texture: Texture,
    size: DVec2,
}

#[derive(Default)]
struct BrowserSurfaceCacheState {
    last_seen_key: Option<BrowserSurfaceCacheKey>,
    stable_repeat_count: u32,
    cached_key: Option<BrowserSurfaceCacheKey>,
    surface: Option<BrowserSurfaceCache>,
}

impl BrowserSurfaceCacheState {
    fn invalidate(&mut self) {
        self.last_seen_key = None;
        self.stable_repeat_count = 0;
        self.cached_key = None;
    }

    fn observe(&mut self, key: BrowserSurfaceCacheKey) {
        if self.last_seen_key == Some(key) {
            self.stable_repeat_count = self.stable_repeat_count.saturating_add(1);
        } else {
            self.last_seen_key = Some(key);
            self.stable_repeat_count = 1;
        }
        if self.cached_key != Some(key) {
            self.cached_key = None;
        }
    }

    fn can_reuse(&self, key: BrowserSurfaceCacheKey) -> bool {
        self.cached_key == Some(key) && self.surface.is_some()
    }

    fn should_promote(&self, key: BrowserSurfaceCacheKey) -> bool {
        self.cached_key != Some(key) && self.stable_repeat_count >= 2
    }
}

static BROWSER_SURFACE_CACHE_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    !matches!(
        std::env::var("HAVI_BROWSER_SURFACE_CACHE"),
        Ok(value) if matches!(value.as_str(), "0" | "false" | "no")
    )
});
static BROWSER_SURFACE_CACHE_STATS_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    matches!(std::env::var("HAVI_RENDER_STATS"), Ok(value) if value == "1")
});

fn hash_browser_scroll_state(scroll_state: &havi_render::ScrollState) -> u64 {
    let mut entries: Vec<_> = scroll_state.iter().collect();
    entries.sort_by_key(|(id, _)| (id.1.0, id.1.1, id.0));
    let mut hasher = DefaultHasher::new();
    for (id, offset) in entries {
        id.hash(&mut hasher);
        offset.x.to_bits().hash(&mut hasher);
        offset.y.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn hash_selection_highlight(selection: Option<&havi_render::SelectionHighlight>) -> u64 {
    let Some(selection) = selection else {
        return 0;
    };
    let mut hasher = DefaultHasher::new();
    selection.color.x.to_bits().hash(&mut hasher);
    selection.color.y.to_bits().hash(&mut hasher);
    selection.color.z.to_bits().hash(&mut hasher);
    selection.color.w.to_bits().hash(&mut hasher);
    for rect in &selection.rects {
        rect.pos.x.to_bits().hash(&mut hasher);
        rect.pos.y.to_bits().hash(&mut hasher);
        rect.size.x.to_bits().hash(&mut hasher);
        rect.size.y.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn ensure_browser_surface_cache<'a>(
    cx: &mut Cx,
    cache_state: &'a mut BrowserSurfaceCacheState,
    size: DVec2,
) -> &'a mut BrowserSurfaceCache {
    let cache = cache_state.surface.get_or_insert_with(|| {
        let pass = DrawPass::new_with_name(cx, "ServoWebViewBrowserSurfaceCache");
        let color_texture = Texture::new_with_format(
            cx,
            TextureFormat::RenderBGRAu8 {
                size: TextureSize::Auto,
                initial: true,
            },
        );
        pass.set_color_texture(
            cx,
            &color_texture,
            DrawPassClearColor::ClearWith(vec4(0.0, 0.0, 0.0, 0.0)),
        );
        BrowserSurfaceCache {
            pass,
            draw_list: DrawList2d::new(cx),
            color_texture,
            size,
        }
    });
    if cache.size != size {
        cache.size = size;
        cache.pass.set_size(cx, size);
        cache_state.cached_key = None;
    }
    cache
}

fn draw_cached_browser_surface(
    draw_cached_surface: &mut DrawCachedSurface,
    cx: &mut Cx2d,
    rect: Rect,
    cache: &BrowserSurfaceCache,
) {
    draw_cached_surface
        .draw_super
        .draw_vars
        .set_texture(0, &cache.color_texture);
    draw_cached_surface.draw_super.draw_abs(cx, rect);
    let area = draw_cached_surface.draw_super.draw_vars.area;
    cx.set_pass_area_with_origin(&cache.pass, area, dvec2(0.0, 0.0));
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Actions emitted by `ServoWebView` in response to user interaction.
///
/// The consuming `App` matches on these to translate into Servo input events.
#[derive(Clone, Debug, Default)]
pub enum ServoWebViewAction {
    #[default]
    None,
    FingerDown {
        abs: DVec2,
        digit_id: u64,
        is_mouse: bool,
        is_right_click: bool,
    },
    FingerUp {
        abs: DVec2,
        digit_id: u64,
        is_mouse: bool,
    },
    FingerMove {
        abs: DVec2,
        digit_id: u64,
        is_mouse: bool,
    },
    HoverIn {
        abs: DVec2,
    },
    HoverOver {
        abs: DVec2,
    },
    HoverOut,
    Scroll {
        abs: DVec2,
        scroll: DVec2,
    },
    KeyDown {
        key_event: KeyEvent,
    },
    KeyUp {
        key_event: KeyEvent,
    },
    TextInput {
        input: String,
        was_paste: bool,
    },
    ClipboardCopyRequested,
    ClipboardCutRequested,
    LongPress {
        abs: DVec2,
    },
    SelectionHandleDrag {
        abs: DVec2,
        handle: makepad_widgets::makepad_platform::SelectionHandleKind,
        phase: makepad_widgets::makepad_platform::SelectionHandlePhase,
    },
}

// ---------------------------------------------------------------------------
// Widget
// ---------------------------------------------------------------------------

/// A Makepad widget that displays Servo's rendered web content as a texture
/// and forwards all touch / mouse / keyboard interaction as widget actions.
///
/// Unlike the stock `Image` widget this calls `event.hits()` in
/// `handle_event`, which registers the draw area for hit-testing so that
/// finger, hover, scroll, and keyboard events are properly captured.
#[derive(Script, ScriptHook, Widget)]
pub struct ServoWebView {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,

    // --- Fragment-based rendering ---
    #[redraw]
    #[live]
    draw_bg: DrawColor,
    #[live]
    draw_content_bg: DrawColor,
    #[live]
    draw_cached_surface: DrawCachedSurface,
    #[rust]
    frame_draw_lists: FrameDrawLists,
    #[rust]
    browser_surface_cache: BrowserSurfaceCacheState,
    #[rust]
    capture_surface_requested: bool,
    /// Shared semantic fragment tree from layout. When set, draw_walk renders
    /// through havi-render's semantic path.
    #[rust]
    shared_layout_fragments: Option<layout_api::SharedLayoutFragmentTree>,
    #[rust]
    shared_webview_id: Option<base::id::WebViewId>,
    /// Data pointer of the last rendered fragment Arc, used to detect when the
    /// fragment tree is replaced (navigation) so GPU caches can be cleared.
    #[rust]
    last_fragment_ptr: usize,
    /// Cached fragment source, rebuilt only when the fragment Arc changes.
    #[rust]
    cached_fragment_source: Option<havi_render::CachedFragmentSource>,

    /// Shared shell scroll snapshot from layout. Used only for DOM-visible
    /// state bootstrap and shell UI diagnostics.
    #[rust]
    shell_scroll_state: Option<layout_api::SharedScrollState>,
    #[rust]
    browser_scroll_controller: BrowserScrollController,

    /// Shared document selection rects from script thread.
    #[rust]
    shared_selection: Option<layout_api::SharedDocumentSelection>,

    /// Shared image source store from Paint. Updated asynchronously with image
    /// data from the network layer and paint-owned producers.
    #[rust]
    image_source_store: Option<paint_api::SharedImageSourceStore>,

    // --- Scroll indicator overlay ---
    #[live]
    draw_scroll_thumb: DrawColor,
    /// Opacity for the scroll indicator (1.0 = visible, fades toward 0).
    #[rust]
    scroll_fade: f64,
}

impl Widget for ServoWebView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        let uid = self.widget_uid();

        let hit = event.hits(cx, self.draw_bg.area());
        match hit {
            // ----- Finger / touch -----
            Hit::FingerDown(fd) => {
                // Request keyboard focus so subsequent key events reach us.
                cx.set_key_focus(self.draw_bg.area());
                let is_right_click = fd.device.mouse_button().is_some_and(|b| b.is_secondary());
                cx.widget_action(
                    uid,
                    ServoWebViewAction::FingerDown {
                        abs: fd.abs,
                        digit_id: fd.digit_id.0.0,
                        is_mouse: matches!(fd.device, DigitDevice::Mouse { .. }),
                        is_right_click,
                    },
                );
            },
            Hit::FingerUp(fu) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::FingerUp {
                        abs: fu.abs,
                        digit_id: fu.digit_id.0.0,
                        is_mouse: matches!(fu.device, DigitDevice::Mouse { .. }),
                    },
                );
            },
            Hit::FingerMove(fm) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::FingerMove {
                        abs: fm.abs,
                        digit_id: fm.digit_id.0.0,
                        is_mouse: matches!(fm.device, DigitDevice::Mouse { .. }),
                    },
                );
            },

            // ----- Hover -----
            Hit::FingerHoverIn(fh) => {
                cx.widget_action(uid, ServoWebViewAction::HoverIn { abs: fh.abs });
            },
            Hit::FingerHoverOver(fh) => {
                cx.widget_action(uid, ServoWebViewAction::HoverOver { abs: fh.abs });
            },
            Hit::FingerHoverOut(_) => {
                cx.widget_action(uid, ServoWebViewAction::HoverOut);
            },

            // ----- Scroll / wheel -----
            Hit::FingerScroll(fs) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::Scroll {
                        abs: fs.abs,
                        scroll: fs.scroll,
                    },
                );
            },

            // ----- Keyboard -----
            Hit::KeyDown(ke) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::KeyDown {
                        key_event: ke,
                    },
                );
            },
            Hit::KeyUp(ke) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::KeyUp {
                        key_event: ke,
                    },
                );
            },

            // ----- Text / IME -----
            Hit::TextInput(ti) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::TextInput {
                        input: ti.input.clone(),
                        was_paste: ti.was_paste,
                    },
                );
            },

            // ----- Clipboard actions -----
            Hit::TextCopy(_) => {
                cx.widget_action(uid, ServoWebViewAction::ClipboardCopyRequested);
            },
            Hit::TextCut(_) => {
                cx.widget_action(uid, ServoWebViewAction::ClipboardCutRequested);
            },

            // ----- Long press -----
            Hit::FingerLongPress(lp) => {
                cx.widget_action(uid, ServoWebViewAction::LongPress { abs: lp.abs });
            },

            // ----- Selection handle drag (mobile) -----
            Hit::SelectionHandleDrag(e) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::SelectionHandleDrag {
                        abs: e.abs,
                        handle: e.handle,
                        phase: e.phase,
                    },
                );
            },

            _ => {},
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let frag_ptr = self
            .shared_layout_fragments
            .as_ref()
            .and_then(|sf| sf.payload_ptr())
            .unwrap_or(0);
        let peek_rect = cx.peek_walk_turtle(walk);

        // Detect fragment tree replacement (navigation) and clear image textures.
        if frag_ptr != self.last_fragment_ptr {
            self.last_fragment_ptr = frag_ptr;
            self.cached_fragment_source = None;
            self.browser_surface_cache.invalidate();
        }

        // Peek at the walk rect BEFORE begin() so we know our expected
        // dimensions even if the inner turtle hasn't resolved sizes yet.
        self.draw_bg.begin(cx, walk, Layout::default());
        // All fragment rendering uses draw_abs (absolute positioning), which
        // doesn't expand the turtle. Mark the full rect as used so
        // draw_bg.end() produces a properly sized area for hit testing.
        // Use the pre-computed peek_rect dimensions, since the inner turtle's
        // rect() may return 0x0 when sizing is not yet resolved.
        cx.turtle_mut().set_used(peek_rect.size.x, peek_rect.size.y);
        self.draw_bg.end(cx);
        let rect = self.draw_bg.area().rect(cx);

        if frag_ptr != 0 {
            // Rebuild stacking context tree only when fragments change.
            let needs_rebuild = self
                .cached_fragment_source
                .as_ref()
                .is_none_or(|c| !c.is_valid_for(frag_ptr));
            if needs_rebuild {
                self.cached_fragment_source =
                    Some(havi_render::CachedFragmentSource::new(frag_ptr));
            }

            // Use the resolved widget area after draw_bg.end(). This gives the
            // render backend a stable target rect and avoids issuing composed
            // browser-content draws while the current pass rect is still 0x0.
            // The visual content is drawn with draw_abs, so it does not depend
            // on the inner turtle remaining open after the hit-test area is
            // established.
            self.browser_scroll_controller.sync_from_layout(
                self.shared_layout_fragments
                    .as_ref()
                    .expect("shared layout fragments"),
                self.shell_scroll_state.as_ref(),
            );
            let render_scroll = self.browser_scroll_controller.render_scroll_state();

            let image_sources = self
                .image_source_store
                .as_ref()
                .map(|s| s.snapshot())
                .unwrap_or_default();

            let selection_highlight = self.shared_selection.as_ref().map(|ss| {
                let snapshot = ss.snapshot();
                havi_render::SelectionHighlight {
                    color: makepad_widgets::makepad_draw::Vec4f {
                        x: 0.26,
                        y: 0.52,
                        z: 0.96,
                        w: 0.4,
                    },
                    rects: snapshot
                        .rects
                        .iter()
                        .map(|r| makepad_widgets::Rect {
                            pos: dvec2(r.origin.x as f64, r.origin.y as f64),
                            size: dvec2(r.size.width as f64, r.size.height as f64),
                        })
                        .collect(),
                }
            });

            let surface_cache_key = if *BROWSER_SURFACE_CACHE_ENABLED && image_sources.is_empty() {
                Some(BrowserSurfaceCacheKey {
                    fragment_ptr: frag_ptr,
                    viewport_width_bits: rect.size.x.to_bits(),
                    viewport_height_bits: rect.size.y.to_bits(),
                    dpi_bits: cx.current_dpi_factor().to_bits(),
                    scroll_hash: hash_browser_scroll_state(&render_scroll),
                    selection_hash: hash_selection_highlight(selection_highlight.as_ref()),
                })
            } else {
                None
            };

            let webview_id = self.shared_webview_id.expect("shared webview id");
            let Some(root_pipeline_id) = self.browser_scroll_controller.root_pipeline_id() else {
                self.capture_surface_requested = false;
                self.draw_scroll_overlay(cx, &rect);
                return DrawStep::done();
            };
            let cached_fragments = havi_render::CachedFragmentSource::new(frag_ptr);

            let capture_requested = self.capture_surface_requested;
            let mut render_into_surface = capture_requested;
            let mut draw_from_surface = capture_requested;
            let mut reusable_surface_key = None;

            if let Some(surface_cache_key) = surface_cache_key {
                self.browser_surface_cache.observe(surface_cache_key);
                reusable_surface_key = Some(surface_cache_key);

                if self.browser_surface_cache.can_reuse(surface_cache_key) {
                    draw_from_surface = true;
                } else if self.browser_surface_cache.should_promote(surface_cache_key) {
                    render_into_surface = true;
                    draw_from_surface = true;
                }
            } else {
                self.browser_surface_cache.invalidate();
            }

            if render_into_surface {
                self.render_into_browser_surface(
                    cx,
                    rect,
                    webview_id,
                    root_pipeline_id,
                    &cached_fragments,
                    &render_scroll,
                    selection_highlight.as_ref(),
                    &image_sources,
                );
                self.browser_surface_cache.cached_key = reusable_surface_key;
            }

            if draw_from_surface {
                let cache = ensure_browser_surface_cache(cx.cx, &mut self.browser_surface_cache, rect.size);
                draw_cached_browser_surface(&mut self.draw_cached_surface, cx, rect, cache);
            } else {
                havi_render::render_fragments_clipped(
                    cx,
                    havi_render::RenderFragmentsClippedParams {
                        webview_id,
                        root_pipeline_id,
                        cached_fragments: &cached_fragments,
                        host_rect: rect,
                        draw_bg: &mut self.draw_content_bg,
                        scroll_state: &render_scroll,
                        selection: selection_highlight.as_ref(),
                        frame_draw_lists: &mut self.frame_draw_lists.0,
                        image_sources: &image_sources,
                    },
                );
            }

            self.capture_surface_requested = false;
        }

        self.draw_scroll_overlay(cx, &rect);

        DrawStep::done()
    }
}

// ---------------------------------------------------------------------------
// Inner helpers
// ---------------------------------------------------------------------------

impl ServoWebView {
    fn render_into_browser_surface(
        &mut self,
        cx: &mut Cx2d,
        rect: Rect,
        webview_id: base::id::WebViewId,
        root_pipeline_id: webrender_api::PipelineId,
        cached_fragments: &havi_render::CachedFragmentSource,
        render_scroll: &havi_render::ScrollState,
        selection_highlight: Option<&havi_render::SelectionHighlight>,
        image_sources: &havi_types::SharedImageSourceMap,
    ) {
        let dpi = cx.current_dpi_factor();
        let draw_content_bg = &mut self.draw_content_bg;
        let frame_draw_lists = &mut self.frame_draw_lists.0;
        let cache = ensure_browser_surface_cache(cx.cx, &mut self.browser_surface_cache, rect.size);
        cache.pass.set_size(cx.cx, rect.size);
        cx.make_child_pass(&cache.pass);
        cx.begin_pass(&cache.pass, Some(dpi));
        cache.draw_list.begin_always(cx);
        cx.begin_root_turtle(rect.size, Layout::flow_down());
        havi_render::render_fragments_clipped(
            cx,
            havi_render::RenderFragmentsClippedParams {
                webview_id,
                root_pipeline_id,
                cached_fragments,
                host_rect: Rect {
                    pos: dvec2(0.0, 0.0),
                    size: rect.size,
                },
                draw_bg: draw_content_bg,
                scroll_state: render_scroll,
                selection: selection_highlight,
                frame_draw_lists,
                image_sources,
            },
        );
        cx.end_pass_sized_turtle();
        cache.draw_list.end(cx);
        cx.end_pass(&cache.pass);
    }

    fn draw_scroll_overlay(&mut self, cx: &mut Cx2d, rect: &Rect) {
        let scroll_state = self
            .shell_scroll_state
            .as_ref()
            .map(|s| s.get())
            .unwrap_or_default();
        if self.scroll_fade > 0.0 && scroll_state.content_height > scroll_state.viewport_height {
            let thumb_width = 4.0;
            let margin_right = 2.0;
            let widget_h = rect.size.y;
            let ratio = scroll_state.viewport_height / scroll_state.content_height;
            let thumb_h = (ratio * widget_h).max(20.0);
            let scroll_range = scroll_state.content_height - scroll_state.viewport_height;
            let thumb_y = if scroll_range > 0.0 {
                (scroll_state.scroll_y / scroll_range) * (widget_h - thumb_h)
            } else {
                0.0
            };
            let alpha = (self.scroll_fade * 0.6) as f32;
            self.draw_scroll_thumb.color = Vec4f {
                x: 0.5,
                y: 0.5,
                z: 0.5,
                w: alpha,
            };
            let thumb_rect = Rect {
                pos: dvec2(
                    rect.pos.x + rect.size.x - thumb_width - margin_right,
                    rect.pos.y + thumb_y,
                ),
                size: dvec2(thumb_width, thumb_h),
            };
            self.draw_scroll_thumb.draw_abs(cx, thumb_rect);
        }
    }
    /// Return the draw area so callers can query geometry (e.g. `area().rect(cx)`).
    pub fn area(&self) -> Area {
        self.draw_bg.area()
    }
}

// ---------------------------------------------------------------------------
// Ref wrapper helpers  (generated by #[derive(Widget)] as ServoWebViewRef)
// ---------------------------------------------------------------------------

impl ServoWebViewRef {
    /// Set the shared browser state for direct Makepad rendering.
    pub fn set_shared_browser_state(
        &self,
        cx: &mut Cx,
        webview_id: base::id::WebViewId,
        root_pipeline_id: Option<webrender_api::PipelineId>,
        shared: layout_api::SharedLayoutFragmentTree,
        scroll_state: layout_api::SharedScrollState,
        selection: layout_api::SharedDocumentSelection,
        image_sources: paint_api::SharedImageSourceStore,
    ) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.shared_webview_id = Some(webview_id);
            inner.browser_scroll_controller.attach_webview(webview_id, root_pipeline_id);
            inner.shared_layout_fragments = Some(shared);
            inner.shell_scroll_state = Some(scroll_state);
            inner.shared_selection = Some(selection);
            inner.image_source_store = Some(image_sources);
            // NOTE: Do NOT clear frame_draw_lists. Makepad's DrawPass pool does
            // not properly clean up freed entries — dropped passes remain in the
            // pool with stale paint_dirty/parent fields, causing cycle panics.
            // Surface passes are reconfigured each frame so reuse is safe.
            // The browser surface cache keys itself by fragment pointer, viewport,
            // scroll state, and selection state. Let draw_walk invalidate it only
            // when the rendered content key actually changes.
            inner.redraw(cx);
        }
    }

    pub fn apply_default_scroll_action(
        &self,
        cx: &mut Cx,
        point: Option<DVec2>,
        delta: DVec2,
    ) -> Option<BrowserScrollCommit> {
        let Some(mut inner) = self.borrow_mut() else {
            return None;
        };
        let shared_fragments = inner.shared_layout_fragments.clone()?;
        let shell_scroll_state = inner.shell_scroll_state.clone();
        inner
            .browser_scroll_controller
            .sync_from_layout(&shared_fragments, shell_scroll_state.as_ref());
        let commit = match point {
            Some(point) => inner
                .browser_scroll_controller
                .apply_scroll_delta_at_point(point, delta)?,
            None => inner.browser_scroll_controller.apply_root_scroll_delta(delta)?,
        };
        inner.redraw(cx);
        Some(commit)
    }

    pub fn prepare_capture_source(&self, cx: &mut Cx) -> Result<CaptureSource, String> {
        let Some(mut inner) = self.borrow_mut() else {
            return Err("webview missing".to_string());
        };
        let area = inner.area();
        if area.is_empty() {
            return Err("webview area unavailable".to_string());
        }
        let rect = area.rect(cx);
        if rect.size.x <= 0.0 || rect.size.y <= 0.0 {
            return Err("webview rect unavailable".to_string());
        }
        if inner
            .shared_layout_fragments
            .as_ref()
            .and_then(|shared| shared.payload_ptr())
            .unwrap_or(0)
            == 0
        {
            return Err("browser fragments unavailable".to_string());
        }

        inner.capture_surface_requested = true;
        let draw_pass_id = {
            let cache = ensure_browser_surface_cache(cx, &mut inner.browser_surface_cache, rect.size);
            cache.pass.draw_pass_id()
        };
        inner.redraw(cx);
        Ok(CaptureSource::CachedView { draw_pass_id })
    }

    /// Convenience accessor for the widget's draw area.
    pub fn area(&self) -> Area {
        if let Some(inner) = self.borrow() {
            inner.area()
        } else {
            Area::Empty
        }
    }

    /// Show the scroll indicator and trigger a redraw.
    pub fn show_scroll_indicator(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.scroll_fade = 1.0;
            inner.redraw(cx);
        }
    }

    /// Decay the scroll indicator opacity. Returns true if still visible.
    pub fn tick_scroll_fade(&self, cx: &mut Cx, dt: f64) -> bool {
        if let Some(mut inner) = self.borrow_mut() {
            if inner.scroll_fade > 0.0 {
                inner.scroll_fade = (inner.scroll_fade - dt * 2.0).max(0.0);
                inner.redraw(cx);
                return inner.scroll_fade > 0.0;
            }
        }
        false
    }
}
