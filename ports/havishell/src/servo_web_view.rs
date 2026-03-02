use std::sync::Arc;

use makepad_widgets::*;

use havi_render::{DrawBoxShadow, DrawFilterImage, DrawGradient, DrawRoundedColor};

// ---------------------------------------------------------------------------
// Widget registration
// ---------------------------------------------------------------------------

script_mod! {
    use mod.prelude.widgets.*

    mod.widgets.ServoWebViewBase = #(ServoWebView::register_widget(vm))
    mod.widgets.ServoWebView = set_type_default() do mod.widgets.ServoWebViewBase{
        width: Fill
        height: Fill
        draw_text.text_style: theme.font_regular
        draw_text_bold.text_style: theme.font_bold
        draw_text_mono.text_style: theme.font_code
    }
}

#[derive(Default)]
struct ImageTextures(havi_render::TextureCache);

#[derive(Default)]
struct ElementScrollState(havi_render::ScrollState);

#[derive(Default)]
struct TransformDrawLists(havi_render::TransformState);

#[derive(Default)]
struct OpacityPasses(havi_render::OpacityState);

#[derive(Default)]
struct FilterPasses(havi_render::FilterState);

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
    draw_text: DrawText,
    #[live]
    draw_text_bold: DrawText,
    #[live]
    draw_text_mono: DrawText,
    #[live]
    draw_rounded_bg: DrawRoundedColor,
    #[live]
    draw_box_shadow: DrawBoxShadow,
    #[live]
    draw_gradient: DrawGradient,
    #[live]
    draw_filter_image: DrawFilterImage,
    #[live]
    draw_image: DrawImage,
    #[rust]
    texture_cache: ImageTextures,
    #[rust]
    element_scroll: ElementScrollState,
    #[rust]
    transform_state: TransformDrawLists,
    #[rust]
    opacity_passes: OpacityPasses,
    #[rust]
    filter_passes: FilterPasses,
    /// Shared fragment tree from layout. When set, draw_walk renders fragments
    /// directly instead of using the GL texture.
    #[rust]
    shared_fragments: Option<layout_api::SharedFragmentTree>,
    /// Data pointer of the last rendered fragment Arc, used to detect when the
    /// fragment tree is replaced (navigation) so GPU caches can be cleared.
    #[rust]
    last_fragment_ptr: usize,
    /// Cached stacking context tree, rebuilt only when the fragment Arc changes.
    /// The tree borrows from `cached_sc_fragments`; the Arc keeps data alive.
    /// SAFETY: `cached_sc_tree` must be dropped/cleared before `cached_sc_fragments`.
    #[rust]
    cached_sc_tree: Option<havi_render::CachedStackingContextTree>,

    /// Shared scroll state from layout. When set, scroll offset and content
    /// height are read from here instead of local estimates.
    #[rust]
    shared_scroll_state: Option<layout_api::SharedScrollState>,

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

        match event.hits(cx, self.draw_bg.area()) {
            // ----- Finger / touch -----
            Hit::FingerDown(fd) => {
                // Request keyboard focus so subsequent key events reach us.
                cx.set_key_focus(self.draw_bg.area());
                let is_right_click = fd.device.mouse_button().map_or(false, |b| b.is_secondary());
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
                        key_event: ke.clone(),
                    },
                );
            },
            Hit::KeyUp(ke) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::KeyUp {
                        key_event: ke.clone(),
                    },
                );
            },

            // ----- Text / IME -----
            Hit::TextInput(ti) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::TextInput {
                        input: ti.input.clone(),
                    },
                );
            },

            _ => {},
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let fragments: Option<Arc<Vec<havi_types::Fragment>>> =
            self.shared_fragments.as_ref().and_then(|sf| sf.get());

        // Detect fragment tree replacement (navigation) and clear image textures.
        let frag_ptr = fragments.as_ref().map_or(0, |f| Arc::as_ptr(f) as usize);
        if frag_ptr != self.last_fragment_ptr {
            self.last_fragment_ptr = frag_ptr;
            self.texture_cache.0.clear();
            // Invalidate cached stacking context tree — will be rebuilt below.
            self.cached_sc_tree = None;
        }

        self.draw_bg.begin(cx, walk, Layout::default());

        if let Some(ref frags) = fragments {
            // Rebuild stacking context tree only when fragments change.
            let needs_rebuild = self
                .cached_sc_tree
                .as_ref()
                .map_or(true, |c| !c.is_valid_for(frags));
            if needs_rebuild {
                self.cached_sc_tree =
                    Some(havi_render::CachedStackingContextTree::new(frags.clone()));
            }

            let rect = cx.turtle().rect();
            // Read scroll state from layout's shared state.
            let scroll_state = self
                .shared_scroll_state
                .as_ref()
                .map(|s| s.get())
                .unwrap_or_default();
            let scroll_y = scroll_state.scroll_y;
            let origin = dvec2(rect.pos.x, rect.pos.y - scroll_y);
            let viewport_top = scroll_y as f32;
            let viewport_bottom = (scroll_y + rect.size.y) as f32;

            havi_render::render_fragments_clipped(
                cx,
                self.cached_sc_tree.as_ref().unwrap(),
                origin,
                viewport_top,
                viewport_bottom,
                &mut self.draw_content_bg,
                &mut self.draw_text,
                &mut self.draw_text_bold,
                &mut self.draw_text_mono,
                &mut self.draw_image,
                &mut self.texture_cache.0,
                &self.element_scroll.0,
                &mut self.draw_rounded_bg,
                &mut self.draw_box_shadow,
                &mut self.draw_gradient,
                None, // selection
                &mut self.transform_state.0,
                &mut self.opacity_passes.0,
                &mut self.filter_passes.0,
                &mut self.draw_filter_image,
            );
        }

        self.draw_bg.end(cx);
        let rect = self.draw_bg.area().rect(cx);
        self.draw_scroll_overlay(cx, &rect);

        DrawStep::done()
    }
}

// ---------------------------------------------------------------------------
// Inner helpers
// ---------------------------------------------------------------------------

impl ServoWebView {
    fn draw_scroll_overlay(&mut self, cx: &mut Cx2d, rect: &Rect) {
        let scroll_state = self
            .shared_scroll_state
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
    /// Set the shared fragment tree and scroll state for direct Makepad rendering.
    pub fn set_shared_fragments(
        &self,
        shared: layout_api::SharedFragmentTree,
        scroll_state: layout_api::SharedScrollState,
    ) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.shared_fragments = Some(shared);
            inner.shared_scroll_state = Some(scroll_state);
            // Clear image textures since they are content-dependent.
            inner.texture_cache.0.clear();
            // NOTE: Do NOT clear opacity_passes, filter_passes, or
            // transform_state. Makepad's DrawPass pool does not properly
            // clean up freed entries — dropped passes remain in the pool
            // with stale paint_dirty/parent fields, causing cycle panics.
            // These passes are reconfigured each frame so reuse is safe.
        }
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
