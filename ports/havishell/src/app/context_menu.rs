use makepad_widgets::*;

use super::App;

/// Context menu dimensions (must match DSL definition).
const MENU_WIDTH: f64 = 160.0 + 8.0; // button width + padding
const ITEM_HEIGHT: f64 = 28.0;
const MENU_PADDING: f64 = 4.0; // top + bottom

/// Build an `hppr-editor://` URL from the current page URL.
/// Returns `None` for non-hppr URLs.
pub(super) fn editor_url_for(url_text: &str) -> Option<String> {
    let url = servo::BrowserUrl::parse(url_text).ok()?;
    if url.scheme() != "hppr" {
        return None;
    }
    Some(format!(
        "hppr-editor://{}",
        &url.as_str()["hppr://".len()..]
    ))
}

impl App {
    /// Show the context menu at the right-click position, populated from Servo's items.
    pub(super) fn show_context_menu(&mut self, cx: &mut Cx) {
        let url_text = self.ui.text_input(cx, ids!(url_input)).text();
        let has_editor = editor_url_for(&url_text).is_some();
        self.ui
            .button(cx, ids!(context_edit_btn))
            .set_visible(cx, has_editor);

        // Compute visible item count for height calculation.
        let mut visible_items = 1; // Copy button always visible
        if has_editor {
            visible_items += 1;
        }
        let menu_height = MENU_PADDING * 2.0 + (visible_items as f64) * ITEM_HEIGHT;

        let content_rect = self.ui.view(cx, ids!(content_area)).area().rect(cx);
        let mut menu_x = self.context_menu_pos.x;
        let mut menu_y = self.context_menu_pos.y;

        let content_right = content_rect.pos.x + content_rect.size.x;
        let content_bottom = content_rect.pos.y + content_rect.size.y;

        if menu_x + MENU_WIDTH > content_right {
            menu_x -= MENU_WIDTH;
        }
        if menu_y + menu_height > content_bottom {
            menu_y -= menu_height;
        }

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
        // Drop the active context menu, which auto-dismisses via Drop impl.
        self.active_context_menu.take();
        self.ui.view(cx, ids!(context_menu)).set_visible(cx, false);
        cx.redraw_all();
    }

    /// Select a context menu action and close the menu.
    pub(super) fn select_context_menu_action(&mut self, cx: &mut Cx, action: servo::ContextMenuAction) {
        if let Some(menu) = self.active_context_menu.take() {
            menu.select(action);
        }
        self.ui.view(cx, ids!(context_menu)).set_visible(cx, false);
        cx.redraw_all();
    }


}
