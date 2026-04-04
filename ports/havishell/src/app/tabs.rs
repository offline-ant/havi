use euclid::Scale;
use makepad_widgets::*;
use libhavi::{DeviceIndependentPixel, DevicePixel, WebViewId};
use webrender_api::PipelineId;
use std::rc::Rc;

use super::{
    dock_button_text, shadow_button_text, watch_button_text, App, HaviWebViewDelegate,
    TabInspectorState,
};

const TAB_MIN_WIDTH: f64 = 120.0;
const TAB_MAX_WIDTH: f64 = 220.0;
const TAB_SCROLL_STEP: f64 = 180.0;

/// Default start page URL.
pub(super) const HOME_URL: &str = "hppr://u/web/index.html";

/// Derive a tab title from a URL. Uses the last path segment.
pub(super) fn title_from_url(url: &str) -> String {
    url.rsplit('/')
        .find(|s| !s.is_empty())
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
    pub(super) root_pipeline_id: Option<PipelineId>,
    pub(super) webview: libhavi::WebView,
    pub(super) animating: bool,
    pub(super) title: String,
    pub(super) url: String,
    pub(super) nav_request_id: u64,
    /// LiveId used as the key in tab_bar View.children.
    pub(super) widget_id: LiveId,
    /// Per-tab HPPR watch state.
    pub(super) watch: libhavi::hppr::watch::WatchHandle,
    pub(super) inspector: TabInspectorState,
}

impl App {
    pub(super) fn active_shadow_enabled(&self) -> bool {
        let Some(tab) = self.tabs.get(self.active_tab_idx) else {
            return false;
        };
        let Ok(addr) = libhavi::hppr::url::HAVIAddress::parse(&tab.url) else {
            return false;
        };
        let parts = addr.parts();
        if parts.group.is_empty() || parts.app.is_empty() || parts.group.starts_with('~') {
            return false;
        }
        libhavi::hppr::state_db::global_state_db()
            .shadow_override_enabled(&parts.group, &parts.app)
            .unwrap_or(false)
    }

    pub(super) fn sync_toolbar_state(&self, cx: &mut Cx) {
        let scope = self
            .tabs
            .get(self.active_tab_idx)
            .map(|tab| tab.watch.scope())
            .unwrap_or_default();
        self.ui
            .button(cx, ids!(watch_btn))
            .set_text(cx, &watch_button_text(scope));
        self.ui
            .button(cx, ids!(shadow_btn))
            .set_text(cx, shadow_button_text(self.active_shadow_enabled()));
        self.ui
            .button(cx, ids!(dock_btn))
            .set_text(cx, dock_button_text(self.menu_at_bottom));
    }

    /// Synchronize the tab bar UI: rebuild children from tab state.
    ///
    /// Tab widgets are created from a live DSL template via `script_from_value`.
    /// The template ScriptObjectRef is extracted once and cached in
    /// `self.tab_template_source`. The template widget is removed from
    /// tab_bar children permanently — keeping it as a hidden child caused
    /// ghost DrawQuad rendering artifacts on Linux/OpenGL.
    pub(super) fn sync_tab_bar(&mut self, cx: &mut Cx) {
        let tab_bar_ref = self.ui.view(cx, ids!(tab_bar));

        // First call: extract and cache the template source, remove from children.
        if self.tab_template_source.is_zero() {
            let source = {
                let tab_bar = tab_bar_ref.borrow_mut();
                tab_bar.and_then(|tb| {
                    tb.children
                        .iter()
                        .find(|(id, _)| *id == live_id!(tab_template))
                        .and_then(|(_, w)| w.borrow_mut::<View>().map(|v| v.source.clone()))
                })
            };
            if let Some(src) = source {
                self.tab_template_source = src;
                if let Some(mut tb) = tab_bar_ref.borrow_mut() {
                    tb.children.retain(|(id, _)| *id != live_id!(tab_template));
                }
            }
        }

        if self.tab_template_source.is_zero() {
            return;
        }

        let template_source = self.tab_template_source.clone();

        let wrap_width = self.ui.view(cx, ids!(tab_bar_wrap)).area().rect(cx).size.x;
        let reserved = 46.0 * 3.0 + 28.0 + 12.0;
        let usable = (wrap_width - reserved).max(120.0);
        let tab_count = self.tabs.len().max(1) as f64;
        let tab_width = (usable / tab_count).clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH);
        let total_tabs_width = tab_width * self.tabs.len() as f64;
        let overflow = total_tabs_width > usable;

        self.ui
            .button(cx, ids!(tab_scroll_left_btn))
            .set_visible(cx, overflow);
        self.ui
            .button(cx, ids!(tab_scroll_right_btn))
            .set_visible(cx, overflow);

        if let Some(mut tab_bar) = self.ui.view(cx, ids!(tab_bar)).borrow_mut() {
            tab_bar.walk.width = if overflow { Size::fill() } else { Size::fit() };
        }

        if !overflow {
            self.tab_scroll_x = 0.0;
        }
        let max_scroll = (total_tabs_width - usable).max(0.0);
        if self.tab_scroll_x > max_scroll {
            self.tab_scroll_x = max_scroll;
        }
        self.ui
            .view(cx, ids!(tab_bar))
            .set_scroll_pos(cx, dvec2(self.tab_scroll_x, 0.0));


        let mut new_children: Vec<(LiveId, WidgetRef)> = Vec::new();

        for (i, tab) in self.tabs.iter().enumerate() {
            let is_active = i == self.active_tab_idx;
            let title = tab.title.clone();
            let widget_id = tab.widget_id;

            let widget = cx.with_vm(|vm| {
                let template_val: ScriptValue = template_source.as_object().into();
                WidgetRef::script_from_value(vm, template_val)
            });

            widget.widget(cx, ids!(tab_label)).set_text(cx, &title);

            let bg: [f32; 4] = if is_active {
                [1.0, 1.0, 1.0, 1.0] // white (active)
            } else {
                [0.96, 0.96, 0.96, 1.0] // light gray (inactive)
            };
            if let Some(mut view) = widget.borrow_mut::<View>() {
                view.walk.width = Size::Fixed(tab_width);
                view.draw_bg.draw_vars.set_uniform(cx, live_id!(color), &bg);
            }

            // Label text color.
            let text_color = if is_active {
                Vec4f {
                    x: 0.067,
                    y: 0.067,
                    z: 0.067,
                    w: 1.0,
                } // #111
            } else {
                Vec4f {
                    x: 0.33,
                    y: 0.33,
                    z: 0.33,
                    w: 1.0,
                } // #555
            };
            if let Some(mut label) = widget.widget(cx, ids!(tab_label)).borrow_mut::<Label>() {
                label.draw_text.color = text_color;
            }

            new_children.push((widget_id, widget));
        }

        // Replace children.
        if let Some(ref mut tab_bar) = tab_bar_ref.borrow_mut() {
            tab_bar.children.clear();
            for entry in new_children {
                tab_bar.children.push(entry);
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
                let Some(tab_idx) = self.tabs.iter().position(|t| t.widget_id == *child_id) else {
                    continue;
                };

                let uid = child_widget.widget_uid();
                if let Some(action) = actions.find_widget_action(uid) {
                    if let ViewAction::FingerDown(fd) = action.cast() {
                        // Middle-click closes the tab immediately without activating it first.
                        if fd.mouse_button().is_some_and(|b| b.is_middle()) {
                            closed_tab = Some(tab_idx);
                            break;
                        }

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
    pub(super) fn create_webview(&self, url_str: &str) -> Option<libhavi::WebView> {
        let servo = self.servo.as_ref()?;
        let url = libhavi::BrowserUrl::parse(url_str).ok()?;
        let hidpi: Scale<f32, DeviceIndependentPixel, DevicePixel> =
            Scale::new(self.dpi_factor as f32);
        let viewport_size = dpi::PhysicalSize::new(self.content_size.0 as u32, self.content_size.1 as u32);
        let webview = libhavi::WebViewBuilder::new(servo, viewport_size)
            .url(url)
            .hidpi_scale_factor(hidpi)
            .delegate(Rc::new(HaviWebViewDelegate))
            .build();
        // Route clipboard through Makepad instead of arboard.
        if let Some(ref state) = self.clipboard_state {
            webview.set_clipboard_delegate(Rc::new(super::clipboard::MakepadClipboardDelegate {
                state: state.clone(),
            }));
        }
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

    pub(super) fn recreate_active_tab_webview(&mut self, cx: &mut Cx) {
        self.sync_content_size_from_host_rect(cx);
        let Some(current) = self.tabs.get(self.active_tab_idx) else {
            return;
        };
        let url = current.url.clone();
        let title = current.title.clone();
        let widget_id = current.widget_id;
        let watch_settings = current.watch.settings();

        let Some(webview) = self.create_webview(&url) else {
            return;
        };
        let webview_id = webview.id();

        let mut watch = libhavi::hppr::watch::WatchHandle::default();
        watch.set_settings(watch_settings);
        self.tabs[self.active_tab_idx] = TabInfo {
            webview_id,
            root_pipeline_id: None,
            webview,
            animating: false,
            title,
            url: url.clone(),
            nav_request_id: current.nav_request_id,
            widget_id,
            watch,
            inspector: current.inspector.clone(),
        };

        self.activate_tab_webview(self.active_tab_idx);
        self.attach_active_browser_state(cx);
        self.focus_active_webview(cx);
        self.set_url_input_sanitized(cx, &url);
        self.sync_toolbar_state(cx);
        self.sync_info_panel(cx);
        self.request_active_page_redraw(cx);
        self.sync_tab_bar(cx);
    }

    pub(super) fn open_tab(&mut self, cx: &mut Cx, url: &str) {
        let Some(webview) = self.create_webview(url) else {
            return;
        };
        let webview_id = webview.id();
        self.tabs.push(TabInfo {
            webview_id,
            root_pipeline_id: None,
            webview,
            animating: false,
            title: title_from_url(url),
            url: url.to_string(),
            nav_request_id: 0,
            widget_id: next_tab_live_id(),
            watch: Default::default(),
            inspector: Default::default(),
        });
        self.active_tab_idx = self.tabs.len() - 1;
        self.activate_tab_webview(self.active_tab_idx);
        self.attach_active_browser_state(cx);
        self.focus_active_webview(cx);
        self.ime_visible = false;
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            self.pending_clipboard_menu = None;
            self.selection_handles_visible = false;
            cx.hide_clipboard_actions();
            cx.hide_selection_handles();
        }
        self.set_url_input_sanitized(cx, url);
        self.sync_toolbar_state(cx);
        self.sync_info_panel(cx);
        self.request_active_page_redraw(cx);
        self.sync_tab_bar(cx);
        self.maybe_start_screenshot_capture(cx);
    }

    /// Open a new home tab and switch to it.
    pub(super) fn open_home_tab(&mut self, cx: &mut Cx) {
        self.sync_content_size_from_host_rect(cx);
        self.open_tab(cx, HOME_URL);
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
        self.active_root_pipeline_id = self.tabs[self.active_tab_idx].root_pipeline_id;
        self.activate_tab_webview(self.active_tab_idx);
        self.ime_visible = false;
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            self.pending_clipboard_menu = None;
            self.selection_handles_visible = false;
            cx.hide_clipboard_actions();
            cx.hide_selection_handles();
        }
        self.attach_active_browser_state(cx);
        self.focus_active_webview(cx);
        let url = self.tabs[self.active_tab_idx].url.clone();
        self.set_url_input_sanitized(cx, &url);
        self.sync_toolbar_state(cx);
        self.sync_info_panel(cx);
        self.request_active_page_redraw(cx);
        self.sync_tab_bar(cx);
    }

    /// Switch to tab at given index.
    pub(super) fn switch_tab(&mut self, cx: &mut Cx, idx: usize) {
        if idx >= self.tabs.len() || idx == self.active_tab_idx {
            return;
        }
        self.active_tab_idx = idx;
        self.active_root_pipeline_id = self.tabs[idx].root_pipeline_id;
        self.activate_tab_webview(idx);
        self.ime_visible = false;
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            self.pending_clipboard_menu = None;
            self.selection_handles_visible = false;
            cx.hide_clipboard_actions();
            cx.hide_selection_handles();
        }
        self.attach_active_browser_state(cx);
        self.focus_active_webview(cx);
        let url = self.tabs[idx].url.clone();
        self.set_url_input_sanitized(cx, &url);
        self.sync_toolbar_state(cx);
        self.sync_info_panel(cx);
        self.request_active_page_redraw(cx);
        self.sync_tab_bar(cx);
    }

    pub(super) fn scroll_tabs(&mut self, cx: &mut Cx, dir: f64) {
        let wrap_width = self.ui.view(cx, ids!(tab_bar_wrap)).area().rect(cx).size.x;
        let reserved = 46.0 * 3.0 + 28.0 + 12.0;
        let usable = (wrap_width - reserved).max(120.0);
        let tab_count = self.tabs.len().max(1) as f64;
        let tab_width = (usable / tab_count).clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH);
        let total_tabs_width = tab_width * self.tabs.len() as f64;
        let max_scroll = (total_tabs_width - usable).max(0.0);

        self.tab_scroll_x = (self.tab_scroll_x + dir * TAB_SCROLL_STEP).clamp(0.0, max_scroll);
        self.ui
            .view(cx, ids!(tab_bar))
            .set_scroll_pos(cx, dvec2(self.tab_scroll_x, 0.0));
        self.ui.view(cx, ids!(tab_bar)).redraw(cx);
    }

    /// Find tab index by webview id.
    pub(super) fn tab_index_for_webview(&self, webview_id: WebViewId) -> Option<usize> {
        self.tabs.iter().position(|t| t.webview_id == webview_id)
    }
}
