use makepad_widgets::*;

use super::{dock_button_text, App};

const MENU_WIDTH: f64 = 220.0;

impl App {
    fn supports_shadow_menu_for_active_tab(&self) -> bool {
        let Some(tab) = self.tabs.get(self.active_tab_idx) else {
            return false;
        };
        let Ok(addr) = libhavi::hppr::url::HAVIAddress::parse(&tab.url) else {
            return false;
        };
        let parts = addr.parts();
        !parts.group.is_empty() && !parts.app.is_empty() && !parts.group.starts_with('~')
    }

    pub(super) fn show_overflow_menu(&mut self, cx: &mut Cx) {
        self.overflow_menu_open = true;
        self.sync_toolbar_state(cx);

        self.ui
            .view(cx, ids!(shadow_control))
            .set_visible(cx, self.supports_shadow_menu_for_active_tab());
        self.ui
            .button(cx, ids!(dock_btn))
            .set_text(cx, dock_button_text(self.menu_at_bottom));

        let button_rect = self.ui.button(cx, ids!(overflow_btn)).area().rect(cx);
        let content_rect = self.ui.view(cx, ids!(content_area)).area().rect(cx);
        let max_x = (content_rect.pos.x + content_rect.size.x - MENU_WIDTH).max(content_rect.pos.x);
        let menu_x = (button_rect.pos.x + button_rect.size.x - MENU_WIDTH)
            .clamp(content_rect.pos.x, max_x);
        let menu_y = if self.menu_at_bottom {
            button_rect.pos.y - 10.0
        } else {
            button_rect.pos.y + button_rect.size.y + 4.0
        };

        let menu = self.ui.view(cx, ids!(overflow_menu));
        menu.set_visible(cx, true);
        if let Some(mut v) = menu.borrow_mut() {
            v.walk.abs_pos = Some(dvec2(menu_x, menu_y));
        }
        cx.redraw_all();
    }

    pub(super) fn hide_overflow_menu(&mut self, cx: &mut Cx) {
        self.overflow_menu_open = false;
        self.ui.view(cx, ids!(overflow_menu)).set_visible(cx, false);
        cx.redraw_all();
    }
}
