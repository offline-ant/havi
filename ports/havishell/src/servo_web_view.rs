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
    FingerDown { abs: DVec2, digit_id: u64 },
    FingerUp { abs: DVec2, digit_id: u64 },
    FingerMove { abs: DVec2, digit_id: u64 },
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
}

impl Widget for ServoWebView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        let uid = self.widget_uid();

        match event.hits(cx, self.draw_bg.area()) {
            // ----- Finger / touch -----
            Hit::FingerDown(fd) => {
                // Request keyboard focus so subsequent key events reach us.
                cx.set_key_focus(self.draw_bg.area());
                cx.widget_action(
                    uid,
                    ServoWebViewAction::FingerDown {
                        abs: fd.abs,
                        digit_id: fd.digit_id.0 .0,
                    },
                );
            }
            Hit::FingerUp(fu) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::FingerUp {
                        abs: fu.abs,
                        digit_id: fu.digit_id.0 .0,
                    },
                );
            }
            Hit::FingerMove(fm) => {
                cx.widget_action(
                    uid,
                    ServoWebViewAction::FingerMove {
                        abs: fm.abs,
                        digit_id: fm.digit_id.0 .0,
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

        self.draw_bg.draw_walk(cx, walk);
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
}
