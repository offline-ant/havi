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
    FingerDown { abs: DVec2, digit_id: u64, is_mouse: bool, is_right_click: bool },
    FingerUp { abs: DVec2, digit_id: u64, is_mouse: bool },
    FingerMove { abs: DVec2, digit_id: u64, is_mouse: bool },
    HoverIn { abs: DVec2 },
    HoverOver { abs: DVec2 },
    HoverOut,
    Scroll { abs: DVec2, scroll: DVec2 },
    KeyDown { key_event: KeyEvent },
    KeyUp { key_event: KeyEvent },
    TextInput { input: String },
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
    #[redraw]
    #[live]
    draw_bg: DrawImage,

    /// The texture produced by Servo's compositor. `None` until the first
    /// frame has been composited.
    #[rust]
    texture: Option<Texture>,

    // --- Scroll indicator overlay ---
    #[live]
    draw_scroll_thumb: DrawColor,
    #[rust]
    scroll_y: f64,
    #[rust]
    content_height: f64,
    #[rust]
    viewport_height: f64,
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
                let is_right_click = fd.device.mouse_button()
                    .map_or(false, |b| b.is_secondary());
                cx.widget_action(
                    uid,
                    ServoWebViewAction::FingerDown {
                        abs: fd.abs,
                        digit_id: fd.digit_id.0 .0,
                        is_mouse: matches!(fd.device, DigitDevice::Mouse { .. }),
                        is_right_click,
                    },
                );
            }
            Hit::FingerUp(fu) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::FingerUp {
                        abs: fu.abs,
                        digit_id: fu.digit_id.0 .0,
                        is_mouse: matches!(fu.device, DigitDevice::Mouse { .. }),
                    },
                );
            }
            Hit::FingerMove(fm) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::FingerMove {
                        abs: fm.abs,
                        digit_id: fm.digit_id.0 .0,
                        is_mouse: matches!(fm.device, DigitDevice::Mouse { .. }),
                    },
                );
            }

            // ----- Hover -----
            Hit::FingerHoverIn(fh) => {
                cx.widget_action(uid, ServoWebViewAction::HoverIn { abs: fh.abs });
            }
            Hit::FingerHoverOver(fh) => {
                cx.widget_action(uid, ServoWebViewAction::HoverOver { abs: fh.abs });
            }
            Hit::FingerHoverOut(_) => {
                cx.widget_action(uid, ServoWebViewAction::HoverOut);
            }

            // ----- Scroll / wheel -----
            Hit::FingerScroll(fs) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::Scroll {
                        abs: fs.abs,
                        scroll: fs.scroll,
                    },
                );
            }

            // ----- Keyboard -----
            Hit::KeyDown(ke) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::KeyDown {
                        key_event: ke.clone(),
                    },
                );
            }
            Hit::KeyUp(ke) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::KeyUp {
                        key_event: ke.clone(),
                    },
                );
            }

            // ----- Text / IME -----
            Hit::TextInput(ti) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::TextInput {
                        input: ti.input.clone(),
                    },
                );
            }

            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        if let Some(ref texture) = self.texture {
            self.draw_bg.draw_vars.set_texture(0, texture);

            // Servo's GL render-target uses bottom-left origin (Y-up).
            // Flip Y so the content is right-side-up in Makepad's Y-down
            // coordinate system.
            self.draw_bg.image_scale = vec2(1.0, -1.0);
            self.draw_bg.image_pan = vec2(0.0, 1.0);
        } else {
            self.draw_bg.draw_vars.empty_texture(0);
        }

        let rect = self.draw_bg.draw_walk(cx, walk);

        // Draw scroll indicator overlay
        if self.scroll_fade > 0.0 && self.content_height > self.viewport_height {
            let thumb_width = 4.0;
            let margin_right = 2.0;
            let widget_h = rect.size.y;
            let ratio = self.viewport_height / self.content_height;
            let thumb_h = (ratio * widget_h).max(20.0);
            let scroll_range = self.content_height - self.viewport_height;
            let thumb_y = if scroll_range > 0.0 {
                (self.scroll_y / scroll_range) * (widget_h - thumb_h)
            } else {
                0.0
            };
            let alpha = (self.scroll_fade * 0.6) as f32;
            self.draw_scroll_thumb.color = Vec4f { x: 0.5, y: 0.5, z: 0.5, w: alpha };
            let thumb_rect = Rect {
                pos: dvec2(
                    rect.pos.x + rect.size.x - thumb_width - margin_right,
                    rect.pos.y + thumb_y,
                ),
                size: dvec2(thumb_width, thumb_h),
            };
            self.draw_scroll_thumb.draw_abs(cx, thumb_rect);
        }

        DrawStep::done()
    }
}

// ---------------------------------------------------------------------------
// Inner helpers
// ---------------------------------------------------------------------------

impl ServoWebView {
    /// Return the draw area so callers can query geometry (e.g. `area().rect(cx)`).
    pub fn area(&self) -> Area {
        self.draw_bg.area()
    }
}

// ---------------------------------------------------------------------------
// Ref wrapper helpers  (generated by #[derive(Widget)] as ServoWebViewRef)
// ---------------------------------------------------------------------------

impl ServoWebViewRef {
    /// Assign (or clear) the texture that this view draws.  Triggers a
    /// redraw when called outside of the draw pass.
    pub fn set_texture(&self, cx: &mut Cx, texture: Option<Texture>) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.texture = texture;
            if cx.in_draw_event() {
                inner.redraw(cx);
            }
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

    /// Update scroll state from a JS query result and trigger redraw.
    pub fn set_scroll_state(&self, cx: &mut Cx, scroll_y: f64, content_height: f64, viewport_height: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.scroll_y = scroll_y;
            inner.content_height = content_height;
            inner.viewport_height = viewport_height;
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
