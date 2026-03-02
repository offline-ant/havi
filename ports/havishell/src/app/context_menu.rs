use makepad_widgets::*;
use makepad_widgets::makepad_platform::event::PopupDismissedEvent;

use super::App;

/// Context menu dimensions (must match DSL definition).
const MENU_WIDTH: f64 = 168.0;
const ITEM_HEIGHT: f64 = 28.0;
const MENU_PADDING: f64 = 8.0; // top + bottom (4 each side)

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
    /// Show the context menu at the right-click position as a popup window.
    pub(super) fn show_context_menu(&mut self, cx: &mut Cx) {
        // Take old handles but keep them alive until after the new popup is
        // allocated.  This prevents the pool allocator from reusing the freed
        // slot and bumping its generation before the deferred CloseWindow op
        // (which still references the old generation) is processed.
        let _old_pass = self.context_popup_pass.take();
        let mut old_window = self.context_popup_window.take();

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
        let menu_height = MENU_PADDING + (visible_items as f64) * ITEM_HEIGHT;

        // Position in parent-client coordinates (from FingerDown abs).
        let parent_window_id = CxWindowPool::id_zero();
        let position = dvec2(self.context_menu_pos.x, self.context_menu_pos.y);
        let size = dvec2(MENU_WIDTH, menu_height);

        let window = WindowHandle::new_popup(cx, parent_window_id, position, size);
        let pass = DrawPass::new(cx);
        pass.set_window_clear_color(cx, vec4(1.0, 1.0, 1.0, 1.0));
        window.set_pass(cx, &pass);

        self.context_popup_window = Some(window);
        self.context_popup_pass = Some(pass);

        // Now safe to close the old popup — new one has a different pool slot.
        if let Some(ref mut w) = old_window {
            w.close(cx);
        }
        // Old handles drop here, freeing pool slots after close ops are queued.

        // The context_menu View stays invisible in the main window tree.
        // It is drawn only into the popup pass by draw_context_menu_popup().
        cx.redraw_all();
    }

    pub(super) fn hide_context_menu(&mut self, cx: &mut Cx) {
        self.active_context_menu.take();
        self.close_context_popup(cx);
        cx.redraw_all();
    }

    /// Close the popup window and clean up state.
    fn close_context_popup(&mut self, cx: &mut Cx) {
        self.ui.view(cx, ids!(context_menu)).set_visible(cx, false);
        if let Some(mut window) = self.context_popup_window.take() {
            window.close(cx);
        }
        self.context_popup_pass = None;
    }

    /// Handle PopupDismissed event from the framework.
    ///
    /// The framework sends this as a notification — the popup is still open.
    /// We must explicitly close it.
    pub(super) fn handle_popup_dismissed(&mut self, cx: &mut Cx, event: &PopupDismissedEvent) {
        if let Some(ref window) = self.context_popup_window {
            if window.window_id() == event.window_id {
                self.hide_context_menu(cx);
            }
        }
    }

    /// Draw context menu contents into the popup pass. Called during draw events.
    ///
    /// The context_menu View stays invisible in the main window tree to avoid
    /// rendering it twice (once in the overlay, once in the popup). We
    /// temporarily set it visible here so `draw_all` produces output, then
    /// restore invisibility before the main window pass draws.
    pub(super) fn draw_context_menu_popup(&mut self, cx: &mut Cx2d) {
        let Some(ref pass) = self.context_popup_pass else {
            return;
        };

        let draw_list = self.context_popup_draw_list.get_or_insert_with(|| DrawList2d::new(cx));

        cx.begin_pass(pass, None);
        draw_list.begin_always(cx);

        let size = cx.current_pass_size();
        cx.begin_root_turtle(size, Layout::flow_down());

        let menu = self.ui.view(cx, ids!(context_menu));
        menu.set_visible(cx, true);
        menu.draw_all(cx, &mut Scope::empty());
        menu.set_visible(cx, false);

        cx.end_pass_sized_turtle();
        draw_list.end(cx);
        cx.end_pass(pass);
    }

    /// Select a context menu action and close the menu.
    pub(super) fn select_context_menu_action(&mut self, cx: &mut Cx, action: servo::ContextMenuAction) {
        if let Some(menu) = self.active_context_menu.take() {
            menu.select(action);
        }
        self.close_context_popup(cx);
        cx.redraw_all();
    }
}
