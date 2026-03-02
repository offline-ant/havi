use makepad_widgets::*;
use servo::{
    CompositionEvent, CompositionState, ImeEvent, Key, KeyState, KeyboardEvent, MouseButton,
    MouseButtonAction, MouseButtonEvent, MouseLeftViewportEvent, NamedKey, TouchEventType, TouchId,
};

use super::{App, TAP_DISTANCE_THRESHOLD};
use crate::servo_web_view::{ServoWebViewAction, ServoWebViewWidgetRefExt};

impl App {
    /// Handle ServoWebView actions (touch/mouse/keyboard/IME input).
    /// Returns true if any input was handled.
    pub(super) fn handle_servo_webview_input(&mut self, cx: &mut Cx, actions: &Actions) -> bool {
        let mut handled_input = false;
        for action in actions {
            if let Some(wa) = action.as_widget_action() {
                let swva: ServoWebViewAction = wa.cast();

                match &swva {
                    ServoWebViewAction::None => {},

                    // ----- Finger / touch -----
                    //
                    // Strategy: defer the Touch(Down) until we know whether
                    // the gesture is a tap or a scroll.
                    //
                    //  • TAP  → send only Mouse events (move + down + up).
                    //           This avoids the double-fire problem where
                    //           both touchend JS listeners AND the synthetic
                    //           click fire on toggle-style UI (hamburger menus).
                    //
                    //  • SCROLL → send Touch(Down) retroactively at the saved
                    //             position, then Touch(Move) for each move,
                    //             and Touch(Up) at the end.
                    //
                    ServoWebViewAction::FingerDown {
                        abs,
                        digit_id: _,
                        is_mouse,
                        is_right_click,
                    } => {
                        if *is_right_click {
                            let pt = self.point_to_device(cx, *abs);
                            self.send_input_event(servo::InputEvent::MouseButton(
                                MouseButtonEvent::new(
                                    MouseButtonAction::Down,
                                    MouseButton::Right,
                                    pt.into(),
                                ),
                            ));
                            self.send_input_event(servo::InputEvent::MouseButton(
                                MouseButtonEvent::new(
                                    MouseButtonAction::Up,
                                    MouseButton::Right,
                                    pt.into(),
                                ),
                            ));
                            self.is_right_click_gesture = true;
                            self.context_menu_pos = *abs;
                            self.context_menu_open = true;
                            self.show_context_menu(cx);
                            handled_input = true;
                        } else {
                            // Close context menu on left click
                            if self.context_menu_open {
                                self.hide_context_menu(cx);
                            }
                            if self.pylon_menu_open {
                                self.hide_pylon_menu(cx);
                            }
                            self.finger_down_pos = Some(*abs);
                            self.is_touch_scrolling = false;
                            self.is_mouse_gesture = *is_mouse;
                            self.is_mouse_dragging = false;
                            // Don't send any event yet — wait to see if it's a tap or drag/scroll.
                            handled_input = true;
                        }
                    },
                    ServoWebViewAction::FingerUp {
                        abs,
                        digit_id,
                        is_mouse: _,
                    } => {
                        if self.is_right_click_gesture {
                            self.is_right_click_gesture = false;
                        } else {
                            let pt = self.point_to_device(cx, *abs);
                            let touch_id = TouchId(*digit_id as i32);
                            if self.is_mouse_dragging {
                                // Complete mouse drag — send final MouseMove + MouseUp
                                self.send_input_event(servo::InputEvent::MouseMove(
                                    servo::MouseMoveEvent::new(pt.into()),
                                ));
                                self.send_input_event(servo::InputEvent::MouseButton(
                                    MouseButtonEvent::new(
                                        MouseButtonAction::Up,
                                        MouseButton::Left,
                                        pt.into(),
                                    ),
                                ));
                            } else if self.is_touch_scrolling {
                                // Complete the touch/scroll sequence
                                self.send_input_event(servo::InputEvent::Touch(
                                    servo::TouchEvent::new(TouchEventType::Up, touch_id, pt.into()),
                                ));
                            } else {
                                // TAP — send mouse click only (no touch events)
                                self.send_input_event(servo::InputEvent::MouseMove(
                                    servo::MouseMoveEvent::new(pt.into()),
                                ));
                                self.send_input_event(servo::InputEvent::MouseButton(
                                    MouseButtonEvent::new(
                                        MouseButtonAction::Down,
                                        MouseButton::Left,
                                        pt.into(),
                                    ),
                                ));
                                self.send_input_event(servo::InputEvent::MouseButton(
                                    MouseButtonEvent::new(
                                        MouseButtonAction::Up,
                                        MouseButton::Left,
                                        pt.into(),
                                    ),
                                ));
                            }
                        }

                        // Reset gesture state
                        self.finger_down_pos = None;
                        self.is_touch_scrolling = false;
                        self.is_mouse_gesture = false;
                        self.is_mouse_dragging = false;
                        handled_input = true;
                    },
                    ServoWebViewAction::FingerMove {
                        abs,
                        digit_id,
                        is_mouse: _,
                    } => {
                        if self.is_right_click_gesture {
                            handled_input = true;
                        } else {
                            let touch_id = TouchId(*digit_id as i32);

                            if !self.is_touch_scrolling && !self.is_mouse_dragging {
                                if let Some(down_pos) = self.finger_down_pos {
                                    let dx = abs.x - down_pos.x;
                                    let dy = abs.y - down_pos.y;
                                    let dist = (dx * dx + dy * dy).sqrt();
                                    if dist > TAP_DISTANCE_THRESHOLD {
                                        if self.is_mouse_gesture {
                                            // Mouse drag — send MouseDown at original position
                                            self.is_mouse_dragging = true;
                                            let down_pt = self.point_to_device(cx, down_pos);
                                            self.send_input_event(servo::InputEvent::MouseButton(
                                                MouseButtonEvent::new(
                                                    MouseButtonAction::Down,
                                                    MouseButton::Left,
                                                    down_pt.into(),
                                                ),
                                            ));
                                        } else {
                                            // Touch scroll
                                            self.is_touch_scrolling = true;
                                            let down_pt = self.point_to_device(cx, down_pos);
                                            self.send_input_event(servo::InputEvent::Touch(
                                                servo::TouchEvent::new(
                                                    TouchEventType::Down,
                                                    touch_id,
                                                    down_pt.into(),
                                                ),
                                            ));
                                        }
                                    }
                                }
                            }

                            if self.is_mouse_dragging {
                                let pt = self.point_to_device(cx, *abs);
                                self.send_input_event(servo::InputEvent::MouseMove(
                                    servo::MouseMoveEvent::new(pt.into()),
                                ));
                            } else if self.is_touch_scrolling {
                                let pt = self.point_to_device(cx, *abs);
                                self.send_input_event(servo::InputEvent::Touch(
                                    servo::TouchEvent::new(
                                        TouchEventType::Move,
                                        touch_id,
                                        pt.into(),
                                    ),
                                ));
                            }
                            handled_input = true;
                        }
                    },

                    // ----- Mouse hover events -----
                    ServoWebViewAction::HoverIn { abs } | ServoWebViewAction::HoverOver { abs } => {
                        let pt = self.point_to_device(cx, *abs);
                        self.send_input_event(servo::InputEvent::MouseMove(
                            servo::MouseMoveEvent::new(pt.into()),
                        ));
                        handled_input = true;
                    },
                    ServoWebViewAction::HoverOut => {
                        self.send_input_event(servo::InputEvent::MouseLeftViewport(
                            MouseLeftViewportEvent::default(),
                        ));
                        handled_input = true;
                    },

                    // ----- Scroll / wheel events -----
                    ServoWebViewAction::Scroll { abs, scroll } => {
                        // Try nested scroll containers first.
                        let web_view_ref = self.ui.servo_web_view(cx, ids!(web_view));
                        let rect = web_view_ref.area().rect(cx);
                        let local_point = dvec2(abs.x - rect.pos.x, abs.y - rect.pos.y);

                        // Read scroll_y from shared state to adjust local_point for root scroll.
                        let scroll_y = web_view_ref.borrow().map_or(0.0, |inner| {
                            inner.root_scroll_y()
                        });
                        let doc_point = dvec2(local_point.x, local_point.y + scroll_y);

                        let nested_consumed = web_view_ref.scroll_nested_container(
                            cx, doc_point, *scroll,
                        );

                        if !nested_consumed {
                            // No nested container consumed it — send to Servo for root scroll.
                            let pt = self.point_to_device(cx, *abs);
                            let delta = servo::WheelDelta {
                                x: scroll.x * self.dpi_factor,
                                y: scroll.y * self.dpi_factor,
                                z: 0.0,
                                mode: servo::WheelMode::DeltaPixel,
                            };
                            self.send_input_event(servo::InputEvent::Wheel(servo::WheelEvent::new(
                                delta,
                                pt.into(),
                            )));
                        }
                        // Show scroll indicator.
                        self.ui
                            .servo_web_view(cx, ids!(web_view))
                            .show_scroll_indicator(cx);
                        handled_input = true;
                    },

                    // ----- Keyboard events -----
                    ServoWebViewAction::KeyDown { key_event } => {
                        // Escape dismisses menus
                        if self.pylon_menu_open {
                            if key_event.key_code
                                == makepad_widgets::makepad_platform::KeyCode::Escape
                            {
                                self.hide_pylon_menu(cx);
                                handled_input = true;
                            }
                        }
                        if self.context_menu_open {
                            if key_event.key_code
                                == makepad_widgets::makepad_platform::KeyCode::Escape
                            {
                                self.hide_context_menu(cx);
                                handled_input = true;
                                continue;
                            }
                        }
                        if let Some(event) = crate::input::translate_key_event(key_event, true) {
                            self.send_input_event(event);
                            handled_input = true;
                        }
                    },
                    ServoWebViewAction::KeyUp { key_event } => {
                        if let Some(event) = crate::input::translate_key_event(key_event, false) {
                            self.send_input_event(event);
                            handled_input = true;
                        }
                    },

                    // ----- IME / text input -----
                    ServoWebViewAction::TextInput { input } => {
                        if !input.is_empty() {
                            self.send_input_event(servo::InputEvent::Keyboard(
                                KeyboardEvent::from_state_and_key(
                                    KeyState::Down,
                                    Key::Named(NamedKey::Process),
                                ),
                            ));
                            self.send_input_event(servo::InputEvent::Ime(ImeEvent::Composition(
                                CompositionEvent {
                                    state: CompositionState::End,
                                    data: input.clone(),
                                },
                            )));
                            self.send_input_event(servo::InputEvent::Keyboard(
                                KeyboardEvent::from_state_and_key(
                                    KeyState::Up,
                                    Key::Named(NamedKey::Process),
                                ),
                            ));
                            handled_input = true;
                        }
                    },
                }
            }
        }
        handled_input
    }
}
