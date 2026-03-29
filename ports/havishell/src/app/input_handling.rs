use makepad_widgets::*;
use servo::{
    CompositionEvent, CompositionState, EditingActionEvent, ImeEvent, Key, KeyState, KeyboardEvent,
    MouseButton, MouseButtonAction, MouseButtonEvent, MouseLeftViewportEvent, NamedKey,
    TouchEventType, TouchId,
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
                            handled_input = true;
                        } else {
                            if self.pylon_menu_open {
                                self.hide_pylon_menu(cx);
                            }
                            if self.overflow_menu_open {
                                self.hide_overflow_menu(cx);
                            }
                            // Dismiss context menu on any non-right-click.
                            // The compositor may also send PopupDismissed,
                            // but that is unreliable (e.g. stale Wayland
                            // serial prevents the popup grab).
                            if self.context_popup_window.is_some() {
                                self.hide_context_menu(cx);
                                handled_input = true;
                                continue;
                            }
                            self.finger_down_pos = Some(*abs);
                            self.is_touch_scrolling = false;
                            self.is_mouse_gesture = *is_mouse;
                            self.is_mouse_dragging = false;
                            #[cfg(any(target_os = "android", target_os = "ios"))]
                            {
                                self.pending_clipboard_menu = None;
                            }
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
                                #[cfg(any(target_os = "android", target_os = "ios"))]
                                {
                                    self.pending_clipboard_menu = None;
                                    cx.hide_clipboard_actions();
                                }
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
                        // Send all wheel events to Servo for DOM dispatch. If the
                        // event is not prevented, paint routes the default action
                        // back into BrowserScrollController after script handling.
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
                        // Show scroll indicator.
                        self.ui
                            .servo_web_view(cx, ids!(web_view))
                            .show_scroll_indicator(cx);
                        handled_input = true;
                    },

                    // ----- Keyboard events -----
                    ServoWebViewAction::KeyDown { key_event } => {
                        if key_event.key_code
                            == makepad_widgets::makepad_platform::KeyCode::Escape
                        {
                            if self.pylon_menu_open {
                                self.hide_pylon_menu(cx);
                                handled_input = true;
                            }
                            if self.overflow_menu_open {
                                self.hide_overflow_menu(cx);
                                handled_input = true;
                            }
                        }
                        if Self::is_primary_new_tab_shortcut(key_event) {
                            self.open_home_tab(cx);
                            handled_input = true;
                        } else {
                            // Suppress primary-modifier clipboard shortcuts.
                            let is_primary_shortcut =
                                key_event.modifiers.control || key_event.modifiers.logo;
                            let is_clipboard_shortcut = matches!(
                                key_event.key_code,
                                makepad_widgets::makepad_platform::KeyCode::KeyC
                                    | makepad_widgets::makepad_platform::KeyCode::KeyX
                                    | makepad_widgets::makepad_platform::KeyCode::KeyV
                            ) && is_primary_shortcut;
                            if is_clipboard_shortcut {
                                handled_input = true;
                            } else if let Some(event) =
                                crate::input::translate_key_event(key_event, true)
                            {
                                self.send_input_event(event);
                                handled_input = true;
                            }
                        }
                    },
                    ServoWebViewAction::KeyUp { key_event } => {
                        if key_event.key_code == makepad_widgets::makepad_platform::KeyCode::KeyT
                            && (key_event.modifiers.control || key_event.modifiers.logo)
                            && !key_event.modifiers.shift
                        {
                            handled_input = true;
                        } else {
                            // Suppress primary-modifier clipboard shortcuts.
                            let is_primary_shortcut =
                                key_event.modifiers.control || key_event.modifiers.logo;
                            let is_clipboard_shortcut = matches!(
                                key_event.key_code,
                                makepad_widgets::makepad_platform::KeyCode::KeyC
                                    | makepad_widgets::makepad_platform::KeyCode::KeyX
                                    | makepad_widgets::makepad_platform::KeyCode::KeyV
                            ) && is_primary_shortcut;
                            if is_clipboard_shortcut {
                                handled_input = true;
                            } else if let Some(event) =
                                crate::input::translate_key_event(key_event, false)
                            {
                                self.send_input_event(event);
                                handled_input = true;
                            }
                        }
                    },

                    // ----- IME / text input -----
                    ServoWebViewAction::TextInput { input, was_paste } => {
                        if *was_paste {
                            // Store paste text for the clipboard delegate, then
                            // trigger Servo's paste editing action.
                            if let Some(ref state) = self.clipboard_state {
                                state.set_pending_paste(input.clone());
                            }
                            self.send_input_event(servo::InputEvent::EditingAction(
                                EditingActionEvent::Paste,
                            ));
                            handled_input = true;
                        } else if !input.is_empty() {
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

                    // ----- Clipboard actions -----
                    ServoWebViewAction::ClipboardCopyRequested => {
                        self.send_input_event(servo::InputEvent::EditingAction(
                            EditingActionEvent::Copy,
                        ));
                        handled_input = true;
                    },
                    ServoWebViewAction::ClipboardCutRequested => {
                        self.send_input_event(servo::InputEvent::EditingAction(
                            EditingActionEvent::Cut,
                        ));
                        handled_input = true;
                    },

                    // ----- Long press (mobile: select word + deferred clipboard actions) -----
                    #[cfg(any(target_os = "android", target_os = "ios"))]
                    ServoWebViewAction::LongPress { abs } => {
                        // Double-click to select word at press point.
                        let pt = self.point_to_device(cx, *abs);
                        self.send_input_event(servo::InputEvent::MouseMove(
                            servo::MouseMoveEvent::new(pt.into()),
                        ));
                        for _ in 0..2 {
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
                        // Defer showing clipboard actions until selection snapshot
                        // has been updated for this press.
                        let baseline_revision = self
                            .tabs
                            .get(self.active_tab_idx)
                            .map(|tab| {
                                layout_api::shared_document_selection_for(tab.webview_id)
                                    .snapshot()
                                    .revision
                            })
                            .unwrap_or(0);
                        self.pending_clipboard_menu = Some(super::PendingClipboardMenu {
                            anchor_abs: *abs,
                            baseline_revision,
                        });
                        cx.hide_clipboard_actions();
                        // Reset gesture state so the finger-up doesn't fire a tap.
                        self.finger_down_pos = None;
                        self.is_touch_scrolling = false;
                        handled_input = true;
                    },
                    #[cfg(not(any(target_os = "android", target_os = "ios")))]
                    ServoWebViewAction::LongPress { .. } => {},

                    // ----- Selection handle drag (mobile) -----
                    ServoWebViewAction::SelectionHandleDrag { abs, handle, phase } => {
                        use makepad_widgets::makepad_platform::SelectionHandlePhase;
                        let pt = self.point_to_device(cx, *abs);
                        match phase {
                            SelectionHandlePhase::Begin => {
                                // Start extending selection from handle position.
                                self.send_input_event(servo::InputEvent::MouseButton(
                                    MouseButtonEvent::new(
                                        MouseButtonAction::Down,
                                        MouseButton::Left,
                                        pt.into(),
                                    ),
                                ));
                            },
                            SelectionHandlePhase::Move => {
                                self.send_input_event(servo::InputEvent::MouseMove(
                                    servo::MouseMoveEvent::new(pt.into()),
                                ));
                            },
                            SelectionHandlePhase::End => {
                                self.send_input_event(servo::InputEvent::MouseButton(
                                    MouseButtonEvent::new(
                                        MouseButtonAction::Up,
                                        MouseButton::Left,
                                        pt.into(),
                                    ),
                                ));
                                // Update handle positions from selection rects.
                                #[cfg(any(target_os = "android", target_os = "ios"))]
                                if let Some(tab) = self.tabs.get(self.active_tab_idx) {
                                    let snapshot =
                                        layout_api::shared_document_selection_for(tab.webview_id)
                                            .snapshot();
                                    if let (Some(first), Some(last)) =
                                        (snapshot.rects.first(), snapshot.rects.last())
                                    {
                                        let start = dvec2(
                                            first.origin.x as f64,
                                            (first.origin.y + first.size.height) as f64,
                                        );
                                        let end = dvec2(
                                            (last.origin.x + last.size.width) as f64,
                                            (last.origin.y + last.size.height) as f64,
                                        );
                                        cx.update_selection_handles(start, end);
                                    }
                                }
                            },
                        }
                        let _ = handle;
                        handled_input = true;
                    },
                }
            }
        }
        handled_input
    }
}
