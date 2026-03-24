use makepad_widgets::*;


// ---------------------------------------------------------------------------
// Widget registration
// ---------------------------------------------------------------------------

script_mod! {
    use mod.prelude.widgets.*

    mod.widgets.ServoWebViewBase = #(ServoWebView::register_widget(vm))
    mod.widgets.ServoWebView = set_type_default() do mod.widgets.ServoWebViewBase{
        width: Fill
        height: Fill
    }
}

#[derive(Default)]
struct FrameDrawLists(havi_render::FrameDrawListState);

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
    #[rust]
    frame_draw_lists: FrameDrawLists,
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

    /// Shared scroll state from layout. When set, scroll offset and content
    /// height are read from here instead of local estimates.
    #[rust]
    shared_scroll_state: Option<layout_api::SharedScrollState>,

    /// Shared document selection rects from script thread.
    #[rust]
    shared_selection: Option<layout_api::SharedDocumentSelection>,

    /// Shared image store from Paint. Updated asynchronously with image data
    /// from the network layer (animated GIF frames, canvas updates, etc.).
    #[rust]
    image_store: Option<paint_api::SharedImageStore>,

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
            let scroll_state = self
                .shared_scroll_state
                .as_ref()
                .map(|s| s.get())
                .unwrap_or_default();

            let render_scroll: havi_render::ScrollState = scroll_state
                .element_offsets
                .iter()
                .map(|(&id, &(x, y))| (id, dvec2(x, y)))
                .collect();

            let image_overrides = self
                .image_store
                .as_ref()
                .map(|s| s.image_overrides())
                .unwrap_or_default();

            havi_render::render_fragments_clipped(
                cx,
                havi_render::RenderFragmentsClippedParams {
                    webview_id: self.shared_webview_id.expect("shared webview id"),
                    cached_fragments: self.cached_fragment_source.as_ref().unwrap(),
                    host_rect: rect,
                    draw_bg: &mut self.draw_content_bg,
                    scroll_state: &render_scroll,
                    selection: self
                        .shared_selection
                        .as_ref()
                        .map(|ss| {
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
                        })
                        .as_ref(),
                    frame_draw_lists: &mut self.frame_draw_lists.0,
                    image_overrides: &image_overrides,
                },
            );
        }

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
    /// Set the shared fragment tree, scroll state, and image store for direct
    /// Makepad rendering.
    pub fn set_shared_layout_fragments(
        &self,
        cx: &mut Cx,
        webview_id: base::id::WebViewId,
        shared: layout_api::SharedLayoutFragmentTree,
        scroll_state: layout_api::SharedScrollState,
        selection: layout_api::SharedDocumentSelection,
        image_store: paint_api::SharedImageStore,
    ) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.shared_webview_id = Some(webview_id);
            inner.shared_layout_fragments = Some(shared);
            inner.shared_scroll_state = Some(scroll_state);
            inner.shared_selection = Some(selection);
            inner.image_store = Some(image_store);
            // NOTE: Do NOT clear frame_draw_lists. Makepad's DrawPass pool does
            // not properly clean up freed entries — dropped passes remain in the
            // pool with stale paint_dirty/parent fields, causing cycle panics.
            // Surface passes are reconfigured each frame so reuse is safe.
            inner.redraw(cx);
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
