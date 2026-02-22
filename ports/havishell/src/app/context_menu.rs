use makepad_widgets::*;
use servo::{Key, KeyboardEvent};

use super::App;

/// Context menu dimensions (must match DSL definition).
const MENU_WIDTH: f64 = 160.0 + 8.0; // button width + padding
const MENU_HEIGHT: f64 = 78.0; // 2 buttons + spacing + padding (measured)

/// Build an `hppr-editor://` URL from the current page URL.
/// Returns `None` for non-hppr URLs.
pub(super) fn editor_url_for(url_text: &str) -> Option<String> {
    let url = url::Url::parse(url_text).ok()?;
    if url.scheme() != "hppr" {
        return None;
    }
    Some(format!("hppr-editor://{}{}", url.host_str().unwrap_or(""), url.path()))
}

impl App {
    /// Show the context menu at the right-click position.
    pub(super) fn show_context_menu(&mut self, cx: &mut Cx) {
        let url_text = self.ui.text_input(cx, ids!(url_input)).text();
        let has_editor = editor_url_for(&url_text).is_some();
        self.ui.button(cx, ids!(context_edit_btn)).set_visible(cx, has_editor);
        // abs_pos is window-absolute in Makepad, so use click position directly.
        let content_rect = self.ui.view(cx, ids!(content_area)).area().rect(cx);
        let mut menu_x = self.context_menu_pos.x;
        let mut menu_y = self.context_menu_pos.y;

        let content_right = content_rect.pos.x + content_rect.size.x;
        let content_bottom = content_rect.pos.y + content_rect.size.y;

        // Flip horizontally if menu would extend past right edge
        if menu_x + MENU_WIDTH > content_right {
            menu_x = menu_x - MENU_WIDTH;
        }
        // Flip vertically if menu would extend past bottom edge
        if menu_y + MENU_HEIGHT > content_bottom {
            menu_y = menu_y - MENU_HEIGHT;
        }

        // Ensure menu stays within content area bounds
        menu_x = menu_x.max(content_rect.pos.x);
        menu_y = menu_y.max(content_rect.pos.y);

        let menu = self.ui.view(cx, ids!(context_menu));
        menu.set_visible(cx, true);
        if let Some(mut v) = menu.borrow_mut() {
            v.walk.abs_pos = Some(dvec2(menu_x, menu_y));
        }
        cx.redraw_all();
    }

    pub(super) fn hide_context_menu(&mut self, cx: &mut Cx) {
        self.context_menu_open = false;
        self.ui.view(cx, ids!(context_menu)).set_visible(cx, false);
        cx.redraw_all();
    }

    /// Send Ctrl+C to Servo to copy selected text.
    pub(super) fn send_copy_command(&self) {
        use keyboard_types::{Code, Modifiers};
        // Send Ctrl+C keydown
        self.send_input_event(servo::InputEvent::Keyboard(
            KeyboardEvent::new(keyboard_types::KeyboardEvent {
                state: keyboard_types::KeyState::Down,
                key: Key::Character("c".into()),
                code: Code::KeyC,
                location: keyboard_types::Location::Standard,
                modifiers: Modifiers::CONTROL,
                repeat: false,
                is_composing: false,
            }),
        ));
        // Send Ctrl+C keyup
        self.send_input_event(servo::InputEvent::Keyboard(
            KeyboardEvent::new(keyboard_types::KeyboardEvent {
                state: keyboard_types::KeyState::Up,
                key: Key::Character("c".into()),
                code: Code::KeyC,
                location: keyboard_types::Location::Standard,
                modifiers: Modifiers::CONTROL,
                repeat: false,
                is_composing: false,
            }),
        ));
    }
}
