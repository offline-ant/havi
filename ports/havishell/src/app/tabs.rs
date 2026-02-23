use makepad_widgets::*;
use euclid::Scale;
use servo::{DeviceIndependentPixel, DevicePixel, WebViewId};
use std::rc::Rc;

use super::{App, HaviWebViewDelegate};

/// Default start page URL.
pub(super) const HOME_URL: &str = "hppr://u/web/index.html";

/// Derive a tab title from a URL. Uses the last path segment.
pub(super) fn title_from_url(url: &str) -> String {
    url.rsplit('/').find(|s| !s.is_empty())
        .unwrap_or("New Tab")
        .to_string()
}

/// Counter for generating unique tab widget LiveIds.
static TAB_ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub(super) fn next_tab_live_id() -> LiveId {
    LiveId(TAB_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

pub(super) struct TabInfo {
    pub(super) webview_id: WebViewId,
    pub(super) webview: servo::WebView,
    pub(super) title: String,
    pub(super) url: String,
    /// LiveId used as the key in tab_bar View.children.
    pub(super) widget_id: LiveId,
    /// Per-tab HPPR watch state.
    pub(super) watch: havi_protocols::watch::WatchHandle,
}

impl App {
    /// Synchronize the tab bar UI: rebuild children from tab state.
    pub(super) fn sync_tab_bar(&mut self, cx: &mut Cx) {
        let tab_bar_ref = self.ui.view(cx, ids!(tab_bar));

        // Extract template source ScriptObjectRef (clone to release borrow)
        let template_source = {
            let tab_bar = tab_bar_ref.borrow_mut();
            tab_bar.and_then(|tb| {
                tb.children.iter()
                    .find(|(id, _)| *id == live_id!(tab_template))
                    .and_then(|(_, w)| {
                        let view_borrow = w.borrow_mut::<View>();
                        view_borrow.map(|v| v.source.clone())
                    })
            })
        };

        let Some(template_source) = template_source else {
            return;
        };

        // Build new tab widgets from the template
        let mut new_children: Vec<(LiveId, WidgetRef)> = Vec::new();

        // Keep the template (hidden)
        {
            if let Some(tb) = tab_bar_ref.borrow_mut() {
                if let Some(entry) = tb.children.iter()
                    .find(|(id, _)| *id == live_id!(tab_template))
                {
                    let entry = entry.clone();
                    entry.1.set_visible(cx, false);
                    new_children.push(entry);
                }
            }
        }

        // Create a tab widget for each tab
        for (i, tab) in self.tabs.iter().enumerate() {
            let is_active = i == self.active_tab_idx;
            let widget = cx.with_vm(|vm| {
                let template_val: ScriptValue = template_source.as_object().into();
                WidgetRef::script_from_value(vm, template_val)
            });
            // Set label text
            widget.widget(cx, ids!(tab_label)).set_text(cx, &tab.title);
            // Set active/inactive bg color
            let bg = if is_active {
                [0.208f32, 0.208, 0.208, 1.0] // #353535
            } else {
                [0.165f32, 0.165, 0.165, 1.0] // #2a2a2a
            };
            // Set bg color via View's draw_bg uniform
            if let Some(mut view) = widget.borrow_mut::<View>() {
                view.draw_bg.draw_vars.set_uniform(cx, live_id!(color), &bg);
            }
            // Set label text color
            let text_color = if is_active {
                Vec4f { x: 0.9, y: 0.9, z: 0.9, w: 1.0 }
            } else {
                Vec4f { x: 0.6, y: 0.6, z: 0.6, w: 1.0 }
            };
            let label_widget = widget.widget(cx, ids!(tab_label));
            if let Some(mut label) = label_widget.borrow_mut::<Label>() {
                label.draw_text.color = text_color;
            }
            new_children.push((tab.widget_id, widget));
        }

        // Preserve the new_tab_btn widget from the original children
        {
            if let Some(tb) = tab_bar_ref.borrow_mut() {
                if let Some(entry) = tb.children.iter()
                    .find(|(id, _)| *id == live_id!(new_tab_btn))
                {
                    new_children.push(entry.clone());
                }
            }
        }

        // Replace children
        {
            let mut tab_bar_borrow = tab_bar_ref.borrow_mut();
            if let Some(ref mut tab_bar) = tab_bar_borrow {
                tab_bar.children.clear();
                for entry in new_children {
                    tab_bar.children.push(entry);
                }
            }
        }
        cx.redraw_all();
    }

    /// Handle clicks on dynamic tab bar children (switch tab / close tab).
    pub(super) fn handle_tab_clicks(&mut self, cx: &mut Cx, actions: &Actions) {
        use makepad_widgets::view::ViewAction;

        let tab_bar_ref = self.ui.view(cx, ids!(tab_bar));
        let mut clicked_tab: Option<usize> = None;
        let mut closed_tab: Option<usize> = None;

        if let Some(tab_bar) = tab_bar_ref.borrow_mut() {
            for (child_id, child_widget) in tab_bar.children.iter() {
                let Some(tab_idx) = self.tabs.iter().position(|t| t.widget_id == *child_id)
                else {
                    continue;
                };

                let uid = child_widget.widget_uid();
                if let Some(action) = actions.find_widget_action(uid) {
                    if let ViewAction::FingerDown(fd) = action.cast() {
                        // Check if click is in the close area (rightmost 24px)
                        let tab_rect = child_widget.area().rect(cx);
                        let close_x = tab_rect.pos.x + tab_rect.size.x - 24.0;
                        if fd.abs.x >= close_x {
                            closed_tab = Some(tab_idx);
                        } else {
                            clicked_tab = Some(tab_idx);
                        }
                        break;
                    }
                }
            }
        }

        if let Some(idx) = closed_tab {
            self.close_tab(cx, idx);
        } else if let Some(idx) = clicked_tab {
            if idx != self.active_tab_idx {
                self.switch_tab(cx, idx);
            }
        }
    }

    /// Create a new Servo WebView for a new tab.
    pub(super) fn create_webview(&self, url_str: &str) -> Option<servo::WebView> {
        let servo = self.servo.as_ref()?;
        let rc = self.rendering_context.as_ref()?;
        let url = servo::BrowserUrl::parse(url_str).ok()?;
        let hidpi: Scale<f32, DeviceIndependentPixel, DevicePixel> =
            Scale::new(self.dpi_factor as f32);
        let webview = servo::WebViewBuilder::new(servo, rc.clone())
            .url(url)
            .hidpi_scale_factor(hidpi)
            .delegate(Rc::new(HaviWebViewDelegate))
            .build();
        // Set size to match current content size
        let (w, h) = self.content_size;
        webview.resize(dpi::PhysicalSize::new(w as u32, h as u32));
        Some(webview)
    }

    /// Activate a tab's webview (show+focus) and deactivate all others.
    pub(super) fn activate_tab_webview(&self, active_idx: usize) {
        for (i, tab) in self.tabs.iter().enumerate() {
            if i == active_idx {
                tab.webview.show();
                tab.webview.focus();
            } else {
                tab.webview.hide();
                tab.webview.blur();
            }
        }
    }

    /// Add a new tab and switch to it.
    pub(super) fn add_tab(&mut self, cx: &mut Cx) {
        let Some(webview) = self.create_webview(HOME_URL) else {
            return;
        };
        let webview_id = webview.id();
        self.tabs.push(TabInfo {
            webview_id,
            webview,
            title: title_from_url(HOME_URL),
            url: HOME_URL.to_string(),
            widget_id: next_tab_live_id(),
            watch: Default::default(),
        });
        self.active_tab_idx = self.tabs.len() - 1;
        self.activate_tab_webview(self.active_tab_idx);
        self.ui.text_input(cx, ids!(url_input)).set_text(cx, HOME_URL);
        self.needs_paint = true;
        self.sync_tab_bar(cx);
    }

    /// Close tab at given index. Quits when the last tab is closed.
    pub(super) fn close_tab(&mut self, cx: &mut Cx, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        if self.tabs.len() <= 1 {
            cx.quit();
            return;
        }
        // Remove the tab (webview is dropped, Servo cleans it up)
        self.tabs.remove(idx);
        if self.active_tab_idx >= self.tabs.len() {
            self.active_tab_idx = self.tabs.len() - 1;
        } else if self.active_tab_idx > idx {
            self.active_tab_idx -= 1;
        }
        // Activate the now-current tab
        self.activate_tab_webview(self.active_tab_idx);
        let url = self.tabs[self.active_tab_idx].url.clone();
        self.ui.text_input(cx, ids!(url_input)).set_text(cx, &url);
        self.needs_paint = true;
        self.sync_tab_bar(cx);
    }

    /// Switch to tab at given index.
    pub(super) fn switch_tab(&mut self, cx: &mut Cx, idx: usize) {
        if idx >= self.tabs.len() || idx == self.active_tab_idx {
            return;
        }
        self.active_tab_idx = idx;
        self.activate_tab_webview(idx);
        let url = self.tabs[idx].url.clone();
        self.ui.text_input(cx, ids!(url_input)).set_text(cx, &url);
        self.ui.button(cx, ids!(watch_btn))
            .set_text(cx, self.tabs[idx].watch.mode().label());
        self.needs_paint = true;
        self.sync_tab_bar(cx);
    }

    /// Find tab index by webview id.
    pub(super) fn tab_index_for_webview(&self, webview_id: WebViewId) -> Option<usize> {
        self.tabs.iter().position(|t| t.webview_id == webview_id)
    }

}
