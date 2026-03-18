use super::*;
use super::navigation::parse_navigation_url;

use havi_protocols::credentials::global_credential_store;
use havi_protocols::resolve;
use havi_protocols::util::mime_from_path;

fn servo_cursor_to_makepad(cursor: servo::Cursor) -> MouseCursor {
    match cursor {
        servo::Cursor::None => MouseCursor::Hidden,
        servo::Cursor::Default => MouseCursor::Default,
        servo::Cursor::Pointer => MouseCursor::Hand,
        servo::Cursor::ContextMenu => MouseCursor::Default,
        servo::Cursor::Help => MouseCursor::Help,
        servo::Cursor::Progress => MouseCursor::Wait,
        servo::Cursor::Wait => MouseCursor::Wait,
        servo::Cursor::Cell => MouseCursor::Crosshair,
        servo::Cursor::Crosshair => MouseCursor::Crosshair,
        servo::Cursor::Text => MouseCursor::Text,
        servo::Cursor::VerticalText => MouseCursor::Text,
        servo::Cursor::Alias => MouseCursor::Default,
        servo::Cursor::Copy => MouseCursor::Default,
        servo::Cursor::Move => MouseCursor::Move,
        servo::Cursor::NoDrop => MouseCursor::NotAllowed,
        servo::Cursor::NotAllowed => MouseCursor::NotAllowed,
        servo::Cursor::Grab => MouseCursor::Arrow,
        servo::Cursor::Grabbing => MouseCursor::Arrow,
        servo::Cursor::EResize => MouseCursor::EResize,
        servo::Cursor::NResize => MouseCursor::NResize,
        servo::Cursor::NeResize => MouseCursor::NeResize,
        servo::Cursor::NwResize => MouseCursor::NwResize,
        servo::Cursor::SResize => MouseCursor::SResize,
        servo::Cursor::SeResize => MouseCursor::SeResize,
        servo::Cursor::SwResize => MouseCursor::SwResize,
        servo::Cursor::WResize => MouseCursor::WResize,
        servo::Cursor::EwResize => MouseCursor::EwResize,
        servo::Cursor::NsResize => MouseCursor::NsResize,
        servo::Cursor::ColResize => MouseCursor::ColResize,
        servo::Cursor::RowResize => MouseCursor::RowResize,
        _ => MouseCursor::Default,
    }
}

fn set_jsonqa_via(current_url: &str, via: &str) -> String {
    if !current_url.ends_with('}') {
        return havi_protocols::url::via_url(current_url, via);
    }

    let Some(start) = current_url.rfind('{') else {
        return havi_protocols::url::via_url(current_url, via);
    };

    let inner = &current_url[start + 1..current_url.len() - 1];
    let mut out_parts: Vec<String> = Vec::new();
    for part in inner.split(',') {
        if part.is_empty() || part.starts_with("via:") {
            continue;
        }
        out_parts.push(part.to_string());
    }
    out_parts.push(format!("via:{}", via));

    let mut out = String::with_capacity(current_url.len() + via.len() + 6);
    out.push_str(&current_url[..start]);
    out.push('{');
    out.push_str(&out_parts.join(","));
    out.push('}');
    out
}

fn shareable_url(current_url: &str, public_via: Option<&str>) -> String {
    let Some(via) = public_via.filter(|v| !v.is_empty()) else {
        return current_url.to_string();
    };

    let Ok(addr) = havi_protocols::url::HAVIAddress::parse(current_url) else {
        return current_url.to_string();
    };

    if !matches!(
        addr.scheme(),
        havi_protocols::url::HpprScheme::Hppr | havi_protocols::url::HpprScheme::HpprBrowse
    ) {
        return current_url.to_string();
    }

    set_jsonqa_via(current_url, via)
}

fn seed_shadow_copy(endpoint: &str, url: &str) -> Result<(), String> {
    let address = havi_protocols::url::HAVIAddress::parse(url).map_err(|e| e.to_string())?;
    if address.is_listing() {
        return Ok(());
    }
    let parts = address.parts();
    if parts.group.is_empty() || parts.app.is_empty() || parts.group.starts_with('~') {
        return Ok(());
    }
    let location = parts.location.clone();
    if location.is_empty() {
        return Ok(());
    }

    let target = hppr_client::parse_via(endpoint).map_err(|e| e.to_string())?;
    let client = std::sync::Arc::new(havi_protocols::client::HpprdClientAsync::new(target)?);
    let creds = global_credential_store();
    let shadow = creds.get_or_create_shadow_credential(&parts.group, &parts.app)?;
    let shadow_group = format!("~{}", parts.group);
    let seed_dir = location.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("").to_string();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("shadow runtime: {}", e))?;

    let copy_file = |runtime: &tokio::runtime::Runtime, path: &str| -> Result<(), String> {
        let file_url = if path.is_empty() {
            format!("hppr://{}/{}/", parts.group, parts.app)
        } else {
            format!("hppr://{}/{}/{}", parts.group, parts.app, path)
        };
        let resolved = runtime.block_on(async { resolve::resolve_document(&file_url, &client, &creds).await })?;
        let content_type = resolved
            .packet
            .header("Content-Type")
            .map(|s| s.to_string())
            .unwrap_or_else(|| mime_from_path(path).to_string());
        let headers = format!(
            "Seal-By: {} {}\nGroup: {}\nApp: {}\nLocation: {}\nContent-Type: {}\n",
            shadow.verification_key,
            shadow.signing_key(),
            shadow_group,
            parts.app,
            path,
            content_type,
        );
        let add_args = hppr_client::build_add_args(headers.as_bytes(), Some(resolved.packet.data()));
        runtime.block_on(async { client.add(&add_args).await })?;
        Ok(())
    };

    let mut copied_any = false;
    let mut dirs = vec![seed_dir.clone()];
    while let Some(dir) = dirs.pop() {
        let list_url = if dir.is_empty() {
            format!("hppr://{}/{}/", parts.group, parts.app)
        } else {
            format!("hppr://{}/{}/{}/", parts.group, parts.app, dir)
        };
        let listing = match runtime.block_on(async { resolve::resolve_listing(&list_url, &client, &creds).await }) {
            Ok(listing) => listing,
            Err(_) if dir == seed_dir => {
                copy_file(&runtime, &location)?;
                copied_any = true;
                break;
            }
            Err(_) => continue,
        };

        for child in listing.children {
            if child == "|/" {
                continue;
            }
            if child.ends_with('/') {
                let child_dir = if dir.is_empty() {
                    child.trim_end_matches('/').to_string()
                } else {
                    format!("{}/{}", dir.trim_end_matches('/'), child.trim_end_matches('/'))
                };
                dirs.push(child_dir);
                continue;
            }
            let path = if dir.is_empty() {
                child
            } else {
                format!("{}/{}", dir.trim_end_matches('/'), child)
            };
            copy_file(&runtime, &path)?;
            copied_any = true;
        }
    }

    if !copied_any {
        copy_file(&runtime, &location)?;
    }
    Ok(())
}

fn enable_shadow_mode(endpoint: &str, url: &str) -> Result<(), String> {
    let address = havi_protocols::url::HAVIAddress::parse(url).map_err(|e| e.to_string())?;
    let parts = address.parts();
    if parts.group.is_empty() || parts.app.is_empty() || parts.group.starts_with('~') {
        return Err("shadow mode requires hppr://<group>/<app>/...".to_string());
    }
    let _ = seed_shadow_copy(endpoint, url);
    havi_protocols::state_db::global_state_db()
        .set_shadow_override(&parts.group, &parts.app, true)
}

fn disable_shadow_mode(url: &str) -> Result<(), String> {
    let address = havi_protocols::url::HAVIAddress::parse(url).map_err(|e| e.to_string())?;
    let parts = address.parts();
    if parts.group.is_empty() || parts.app.is_empty() || parts.group.starts_with('~') {
        return Err("shadow mode requires hppr://<group>/<app>/...".to_string());
    }
    havi_protocols::state_db::global_state_db()
        .set_shadow_override(&parts.group, &parts.app, false)
}

impl App {
    fn toggle_shadow_for_active_tab(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab_idx) else {
            return;
        };
        let Ok(addr) = havi_protocols::url::HAVIAddress::parse(&tab.url) else {
            return;
        };
        let parts = addr.parts();
        if parts.group.is_empty() || parts.app.is_empty() || parts.group.starts_with('~') {
            return;
        }

        let enable = !havi_protocols::state_db::global_state_db()
            .shadow_override_enabled(&parts.group, &parts.app)
            .unwrap_or(false);
        let url = tab.url.clone();
        let webview_id = tab.webview_id;
        let endpoint = self.watch_fallback_endpoint.clone();

        std::thread::Builder::new()
            .name("havi-shadow".to_string())
            .spawn(move || {
                let result = if enable {
                    enable_shadow_mode(&endpoint, &url)
                } else {
                    disable_shadow_mode(&url)
                };
                let error = result.err();
                Cx::post_action(MakepadServoAction::ShadowModeSet {
                    webview_id,
                    enabled: enable && error.is_none(),
                    error,
                });
                SignalToUI::set_ui_signal();
            })
            .ok();
    }

    fn complete_startup_navigation(&mut self, cx: &mut Cx) {
        if self.start_navigation_done {
            return;
        }
        self.start_navigation_done = true;

        // Hide splash screen first so content_area has its final startup geometry.
        self.ui.view(cx, ids!(splash_screen)).set_visible(cx, false);
        self.sync_content_size_from_host_rect(cx);

        // Create the first webview if none exists yet (splash screen path).
        if self.tabs.is_empty() {
            if let Some(webview) = self.create_webview(&self.start_url) {
                let webview_id = webview.id();
                self.tabs.push(TabInfo {
                    webview_id,
                    root_pipeline_id: None,
                    webview,
                    title: title_from_url(&self.start_url),
                    url: self.start_url.clone(),
                    widget_id: next_tab_live_id(),
                    watch: Default::default(),
                });
                self.active_tab_idx = 0;
                self.attach_active_render_state(cx);
                self.activate_tab_webview(0);
                self.focus_active_webview(cx);
                #[cfg(any(target_os = "android", target_os = "ios"))]
                {
                    self.pending_clipboard_menu = None;
                    self.selection_handles_visible = false;
                    cx.hide_clipboard_actions();
                    cx.hide_selection_handles();
                }
            }
        } else {
            self.navigate(&self.start_url);
            if let Some(tab) = self.tabs.get_mut(self.active_tab_idx) {
                tab.url = self.start_url.clone();
                tab.title = title_from_url(&self.start_url);
            }
        }

        // Splash is already hidden above. Show chrome.
        self.ui
            .text_input(cx, ids!(url_input))
            .set_text(cx, &self.start_url);
        self.sync_toolbar_state(cx);
        self.sync_tab_bar(cx);
        self.maybe_start_screenshot_capture(cx);
    }

    pub(super) fn apply_menu_dock(&self, cx: &mut Cx) {
        let tab_uid = self.ui.view(cx, ids!(tab_bar_wrap)).widget_uid();
        let toolbar_uid = self.ui.view(cx, ids!(toolbar)).widget_uid();
        let content_uid = self.ui.view(cx, ids!(content_area)).widget_uid();

        if let Some(mut main_layout) = self.ui.view(cx, ids!(main_layout)).borrow_mut() {
            main_layout.children.sort_by_key(|(_, child)| {
                let uid = child.widget_uid();
                if self.menu_at_bottom {
                    if uid == content_uid {
                        0
                    } else if uid == toolbar_uid {
                        1
                    } else if uid == tab_uid {
                        2
                    } else {
                        3
                    }
                } else if uid == tab_uid {
                    0
                } else if uid == toolbar_uid {
                    1
                } else if uid == content_uid {
                    2
                } else {
                    3
                }
            });
        }

        self.ui.view(cx, ids!(main_layout)).redraw(cx);
    }
}

impl MatchEvent for App {
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let mut nav_action: Option<NavCommand> = None;

        if self.ui.button(cx, ids!(back_btn)).clicked(actions) {
            nav_action = Some(NavCommand::Back);
        }
        if self.ui.button(cx, ids!(forward_btn)).clicked(actions) {
            nav_action = Some(NavCommand::Forward);
        }
        if self.ui.button(cx, ids!(reload_btn)).clicked(actions) {
            nav_action = Some(NavCommand::Reload);
        }
        if self.ui.button(cx, ids!(go_btn)).clicked(actions) {
            let url_text = self.ui.text_input(cx, ids!(url_input)).text();
            nav_action = Some(NavCommand::Navigate(url_text));
        }
        if self.ui.button(cx, ids!(edit_btn)).clicked(actions) {
            let url_text = self.ui.text_input(cx, ids!(url_input)).text();
            if let Some(edit_url) = context_menu::editor_url_for(&url_text) {
                nav_action = Some(NavCommand::Navigate(edit_url));
            }
        }
        if self.ui.button(cx, ids!(watch_btn)).clicked(actions) {
            if let Some(tab) = self.tabs.get_mut(self.active_tab_idx) {
                let next_scope = tab.watch.scope().next();
                tab.watch.set_scope(next_scope);
                if next_scope != havi_protocols::watch::WatchScope::None {
                    self.ensure_watch_pool();
                }
                self.ui
                    .button(cx, ids!(watch_btn))
                    .set_text(cx, &watch_button_text(next_scope));
            }
        }
        if self.ui.button(cx, ids!(shadow_btn)).clicked(actions) {
            self.toggle_shadow_for_active_tab();
        }
        if self.ui.button(cx, ids!(share_btn)).clicked(actions) {
            let input_url = self.ui.text_input(cx, ids!(url_input)).text();
            let effective_url = self
                .tabs
                .get(self.active_tab_idx)
                .map(|tab| tab.url.as_str())
                .unwrap_or(input_url.as_str());
            let share_url = shareable_url(effective_url, self.shared_public_via.as_deref());
            cx.copy_to_clipboard(&share_url);
        }
        if self.ui.button(cx, ids!(home_btn)).clicked(actions) {
            nav_action = Some(NavCommand::Navigate(HOME_URL.into()));
        }
        if self.ui.button(cx, ids!(dock_btn)).clicked(actions) {
            self.menu_at_bottom = !self.menu_at_bottom;
            let text = if self.menu_at_bottom { "🔽" } else { "🔼" };
            self.ui.button(cx, ids!(dock_btn)).set_text(cx, text);
            self.apply_menu_dock(cx);
            self.needs_paint = true;
            self.idle_frames = 0;
            self.next_frame = cx.new_next_frame();
            cx.redraw_all();
        }
        if self
            .ui
            .text_input(cx, ids!(url_input))
            .returned(actions)
            .is_some()
        {
            let url_text = self.ui.text_input(cx, ids!(url_input)).text();
            nav_action = Some(NavCommand::Navigate(url_text));
        }

        // --- Window control buttons ---
        if self.ui.button(cx, ids!(win_min)).clicked(actions) {
            cx.push_unique_platform_op(CxOsOp::MinimizeWindow(CxWindowPool::id_zero()));
        }
        if self.ui.button(cx, ids!(win_max)).clicked(actions) {
            let is_fs = cx.windows[CxWindowPool::id_zero()]
                .window_geom
                .is_fullscreen;
            if is_fs {
                cx.push_unique_platform_op(CxOsOp::RestoreWindow(CxWindowPool::id_zero()));
            } else {
                cx.push_unique_platform_op(CxOsOp::MaximizeWindow(CxWindowPool::id_zero()));
            }
        }
        if self.ui.button(cx, ids!(win_close)).clicked(actions) {
            cx.quit();
        }

        // --- Context menu ---
        if let Some(cmd) = self.handle_context_menu_actions(cx, actions) {
            nav_action = Some(cmd);
        }

        // --- Pylon dot click ---
        if self
            .ui
            .view(cx, ids!(pylon_dot))
            .finger_down(actions)
            .is_some()
        {
            if self.pylon_menu_open {
                self.hide_pylon_menu(cx);
            } else {
                // Refresh status before showing.
                self.refresh_pylon_status(cx);
                self.show_pylon_menu(cx);
            }
        }

        // --- Pylon menu buttons ---
        if self
            .ui
            .button(cx, ids!(pylon_hpprd_start_btn))
            .clicked(actions)
        {
            self.hide_pylon_menu(cx);
            self.pylon_command(cx, "start", Some("hpprd"), None);
        }
        if self
            .ui
            .button(cx, ids!(pylon_hpprd_stop_btn))
            .clicked(actions)
        {
            self.hide_pylon_menu(cx);
            self.pylon_command(cx, "stop", Some("hpprd"), None);
        }
        if self
            .ui
            .button(cx, ids!(pylon_nfs_start_btn))
            .clicked(actions)
        {
            self.hide_pylon_menu(cx);
            self.pylon_command(cx, "start", Some("hppr-nfs"), None);
        }
        if self
            .ui
            .button(cx, ids!(pylon_nfs_stop_btn))
            .clicked(actions)
        {
            self.hide_pylon_menu(cx);
            self.pylon_command(cx, "stop", Some("hppr-nfs"), None);
        }
        if self.ui.button(cx, ids!(pylon_mount_btn)).clicked(actions) {
            self.hide_pylon_menu(cx);
            self.pylon_command(cx, "mount", None, None);
        }
        if self.ui.button(cx, ids!(pylon_unmount_btn)).clicked(actions) {
            self.hide_pylon_menu(cx);
            self.pylon_command(cx, "unmount", None, None);
        }
        if self
            .ui
            .button(cx, ids!(pylon_services_btn))
            .clicked(actions)
        {
            self.hide_pylon_menu(cx);
            nav_action = Some(NavCommand::Navigate("havi:///services".into()));
        }
        if self
            .ui
            .button(cx, ids!(pylon_shutdown_btn))
            .clicked(actions)
        {
            self.hide_pylon_menu(cx);
            self.pylon_command(cx, "shutdown", None, None);
            self.pylon_status.health = pylon_menu::PylonHealth::Red;
            self.pylon_command_client = None;
            self.update_pylon_dot(cx);
        }

        // --- Tab bar events ---
        if self
            .ui
            .button(cx, ids!(tab_scroll_left_btn))
            .clicked(actions)
        {
            self.scroll_tabs(cx, -1.0);
        }
        if self
            .ui
            .button(cx, ids!(tab_scroll_right_btn))
            .clicked(actions)
        {
            self.scroll_tabs(cx, 1.0);
        }
        if self.ui.button(cx, ids!(new_tab_btn)).clicked(actions) {
            self.add_tab(cx);
            // Reset cursor — the button moves when a tab is added, so
            // the hover-out event may not fire, leaving cursor stuck as Hand.
            cx.set_cursor(MouseCursor::Default);
        }

        // Tab click/close: detect finger-down on dynamic tab children.
        // We iterate tab_bar children and match by widget_id.
        self.handle_tab_clicks(cx, actions);

        // Apply navigation command
        if let Some(ref cmd) = nav_action {
            match cmd {
                NavCommand::Back => self.go_back(),
                NavCommand::Forward => self.go_forward(),
                NavCommand::Reload => self.reload(),
                NavCommand::Navigate(url) => {
                    self.navigate(url);
                    // Update active tab URL
                    if let Some(tab) = self.tabs.get_mut(self.active_tab_idx) {
                        tab.url = url.clone();
                    }
                },
            }
        }

        if let Some(servo) = &self.servo {
            for request in servo.paint_screenshot_bridge().drain_requests() {
                if self
                    .tabs
                    .get(self.active_tab_idx)
                    .map(|tab| tab.webview_id == request.webview_id)
                    .unwrap_or(false)
                {
                    // SCREENSHOT CAPTURE FOR SERVO WEBVIEW CONTENT MUST COME FROM THE CACHED
                    // WEBVIEW SURFACE.
                    // DO NOT EVER SWAP THIS FOR FRAMEBUFFER.
                    // FRAMEBUFFER CAPTURE INCLUDES HAVI SHELL CHROME AND IS THE WRONG DATA
                    // SOURCE FOR DEVTOOLS / WEBVIEW SCREENSHOTS.
                    // THE CORRECT PATH IS CACHED-VIEW CAPTURE, SO HAVI REQUESTS CAPTURE FROM
                    // THE CACHED WEBVIEW SURFACE WITHOUT TOUCHING TEXTURE OR FRAMEBUFFER
                    // PLUMBING.
                    let source = match self
                        .ui
                        .view(cx, ids!(web_view_texture))
                        .cached_capture_source()
                    {
                        Ok(source) => source,
                        Err(_) => {
                            continue;
                        }
                    };
                    let capture_request_id = cx.request_capture(source);
                    self.pending_screenshot_callbacks
                        .insert(capture_request_id, (request.webview_id, request.request_id));
                    self.next_frame = cx.new_next_frame();
                    cx.redraw_all();
                }
            }
        }

        // Handle Servo actions (Wake + WebView delegate events)
        for action in actions {
            match action.downcast_ref::<MakepadServoAction>() {
                Some(MakepadServoAction::Wake) => {
                    self.needs_paint = true;
                    self.idle_frames = 0;
                    self.next_frame = cx.new_next_frame();
                    cx.redraw_all();
                },
                Some(MakepadServoAction::TitleChanged { webview_id, title }) => {
                    let webview_id = *webview_id;
                    let title = title.clone();
                    if let Some(idx) = self.tab_index_for_webview(webview_id) {
                        self.tabs[idx].title = title
                            .clone()
                            .unwrap_or_else(|| title_from_url(&self.tabs[idx].url));
                        self.sync_tab_bar(cx);
                    }
                },
                Some(MakepadServoAction::UrlChanged { webview_id, url }) => {
                    let webview_id = *webview_id;
                    let url = url.clone();
                    if let Some(idx) = self.tab_index_for_webview(webview_id) {
                        self.tabs[idx].url = url.clone();
                        self.tabs[idx].watch.clear_change_detected();
                        if idx == self.active_tab_idx {
                            self.ui.text_input(cx, ids!(url_input)).set_text(cx, &url);
                            self.sync_toolbar_state(cx);
                        }

                        let title = self.tabs[idx].title.clone();
                        if let Err(e) =
                            havi_protocols::state_db::global_state_db().insert_history(&url, &title)
                        {
                            log!("[havi] failed to persist history entry: {}", e);
                        }
                    }
                },
                Some(MakepadServoAction::LoadStatusChanged { webview_id, status }) => {
                    let webview_id = *webview_id;
                    let status = *status;
                    if self
                        .tabs
                        .get(self.active_tab_idx)
                        .map_or(false, |t| t.webview_id == webview_id)
                        && status == servo::LoadStatus::Complete
                    {
                        self.focus_active_webview(cx);
                        self.maybe_start_screenshot_capture(cx);
                    }
                },
                Some(MakepadServoAction::NewFrameReady {
                    webview_id,
                    pipeline_id,
                }) => {
                    let webview_id = *webview_id;
                    let pipeline_id = *pipeline_id;
                    // Only repaint if the active webview has new content
                    if let Some(idx) = self.tab_index_for_webview(webview_id) {
                        self.tabs[idx].root_pipeline_id = Some(pipeline_id);
                        if idx == self.active_tab_idx {
                            self.active_root_pipeline_id = Some(pipeline_id);
                            self.attach_active_render_state(cx);
                            self.needs_paint = true;
                            self.idle_frames = 0;
                            self.next_frame = cx.new_next_frame();
                            cx.redraw_all();
                        }
                    }
                },
                Some(MakepadServoAction::CursorChanged { webview_id, cursor }) => {
                    let webview_id = *webview_id;
                    let cursor = *cursor;
                    if self
                        .tabs
                        .get(self.active_tab_idx)
                        .map_or(false, |t| t.webview_id == webview_id)
                    {
                        let makepad_cursor = servo_cursor_to_makepad(cursor);
                        cx.set_cursor(makepad_cursor);
                    }
                },
                Some(MakepadServoAction::WebViewClosed { webview_id }) => {
                    let webview_id = *webview_id;
                    if let Some(idx) = self.tab_index_for_webview(webview_id) {
                        self.close_tab(cx, idx);
                    }
                },
                Some(MakepadServoAction::ImeShow { webview_id }) => {
                    let webview_id = *webview_id;
                    if self
                        .tabs
                        .get(self.active_tab_idx)
                        .map_or(false, |t| t.webview_id == webview_id)
                    {
                        self.ime_visible = true;
                        let web_view = self.ui.servo_web_view(cx, ids!(web_view));
                        let area = web_view.area();
                        let rect = area.rect(cx);
                        cx.show_text_ime(area, dvec2(rect.pos.x, rect.pos.y + rect.size.y));
                    }
                },
                Some(MakepadServoAction::ImeHide { webview_id }) => {
                    let webview_id = *webview_id;
                    if self
                        .tabs
                        .get(self.active_tab_idx)
                        .map_or(false, |t| t.webview_id == webview_id)
                    {
                        self.ime_visible = false;
                        cx.hide_text_ime();
                    }
                },
                Some(MakepadServoAction::ContextMenuShow {
                    webview_id,
                    context_menu,
                }) => {
                    let webview_id = *webview_id;
                    if self
                        .tabs
                        .get(self.active_tab_idx)
                        .map_or(false, |t| t.webview_id == webview_id)
                    {
                        if let Some(menu) = context_menu.lock().unwrap().take() {
                            self.last_context_menu_flags = Some(menu.element_info().flags);
                            self.active_context_menu = Some(menu);
                            self.show_context_menu(cx);
                        }
                    }
                },
                Some(MakepadServoAction::WatchGetMode {
                    webview_id,
                    response_sender,
                }) => {
                    let wire = self
                        .tab_index_for_webview(*webview_id)
                        .and_then(|idx| self.tabs.get(idx))
                        .map(|tab| settings_to_wire(tab.watch.settings()))
                        .unwrap_or_else(|| "none".to_string());
                    let _ = response_sender.send(wire);
                },
                Some(MakepadServoAction::WatchSetMode {
                    webview_id,
                    mode,
                    response_sender,
                }) => {
                    let tab_idx = self.tab_index_for_webview(*webview_id);
                    let mut need_watch_pool = false;
                    let new_wire = if let Some(settings) = settings_from_wire(mode) {
                        if settings.is_active() {
                            need_watch_pool = true;
                        }
                        if let Some(idx) = tab_idx {
                            if let Some(tab) = self.tabs.get_mut(idx) {
                                tab.watch.set_settings(settings);
                                if idx == self.active_tab_idx {
                                    self.ui
                                        .button(cx, ids!(watch_btn))
                                        .set_text(cx, &watch_button_text(tab.watch.scope()));
                                }
                                settings_to_wire(tab.watch.settings())
                            } else {
                                "none".to_string()
                            }
                        } else {
                            "none".to_string()
                        }
                    } else {
                        tab_idx
                            .and_then(|idx| self.tabs.get(idx))
                            .map(|tab| settings_to_wire(tab.watch.settings()))
                            .unwrap_or_else(|| "none".to_string())
                    };
                    if need_watch_pool {
                        self.ensure_watch_pool();
                    }
                    let _ = response_sender.send(new_wire);
                },
                Some(MakepadServoAction::DevtoolsSetUrl {
                    webview_id,
                    url,
                    response_sender,
                }) => {
                    let response = if let Some(idx) = self.tab_index_for_webview(*webview_id) {
                        if let Some(parsed) = parse_navigation_url(url) {
                            let parsed_url = parsed.to_string();
                            self.tabs[idx].webview.load(parsed);
                            self.tabs[idx].url = parsed_url.clone();
                            if idx == self.active_tab_idx {
                                self.attach_active_render_state(cx);
                                self.focus_active_webview(cx);
                                self.ui.text_input(cx, ids!(url_input)).set_text(cx, &parsed_url);
                            }
                            self.sync_tab_bar(cx);
                            self.needs_paint = true;
                            self.idle_frames = 0;
                            self.next_frame = cx.new_next_frame();
                            Ok(parsed_url)
                        } else {
                            Err("invalid url".to_string())
                        }
                    } else {
                        Err("unknown webview".to_string())
                    };
                    let _ = response_sender.send(response);
                },
                Some(MakepadServoAction::DevtoolsActivateWebView {
                    webview_id,
                    response_sender,
                }) => {
                    let response = if let Some(idx) = self.tab_index_for_webview(*webview_id) {
                        self.switch_tab(cx, idx);
                        self.needs_paint = true;
                        self.idle_frames = 0;
                        self.next_frame = cx.new_next_frame();
                        Ok(())
                    } else {
                        Err("unknown webview".to_string())
                    };
                    let _ = response_sender.send(response);
                },
                Some(MakepadServoAction::AccessibilityUpdate { webview_id, update }) => {
                    let webview_id = *webview_id;
                    if self
                        .tabs
                        .get(self.active_tab_idx)
                        .map_or(false, |t| t.webview_id == webview_id)
                    {
                        if let Some(tree_update) = update.lock().unwrap().take() {
                            cx.update_accessibility_tree(Box::new(tree_update));
                        }
                    }
                },
                Some(MakepadServoAction::CameraRequest(request)) => {
                    if let Some(request) = request.lock().unwrap().take() {
                        self.camera.handle_request(cx, request);
                    }
                },
                Some(MakepadServoAction::ShadowModeSet {
                    webview_id,
                    enabled,
                    error,
                }) => {
                    if let Some(err) = error.as_ref() {
                        log!("[havi] shadow mode error for {:?}: {}", webview_id, err);
                    }
                    if let Some(idx) = self.tab_index_for_webview(*webview_id) {
                        if *enabled {
                            if let Some(tab) = self.tabs.get_mut(idx) {
                                tab.watch.set_settings(havi_protocols::watch::WatchSettings {
                                    scope: havi_protocols::watch::WatchScope::App,
                                    navigate: true,
                                });
                            }
                            self.ensure_watch_pool();
                        }
                        if self
                            .tabs
                            .get(self.active_tab_idx)
                            .map(|tab| tab.webview_id == *webview_id)
                            .unwrap_or(false)
                        {
                            self.sync_toolbar_state(cx);
                            self.recreate_active_tab_webview(cx);
                        }
                    }
                },
                _ => {},
            }
        }

        // Drain clipboard copy queue.
        if let Some(ref state) = self.clipboard_state {
            let actions_queued: Vec<_> = state.action_queue.borrow_mut().drain(..).collect();
            for action in actions_queued {
                match action {
                    clipboard::ClipboardAction::Copy(text) => cx.copy_to_clipboard(&text),
                }
            }
        }

        // Handle ServoWebView actions (touch/mouse/keyboard/IME input)
        let handled_input = self.handle_servo_webview_input(cx, actions);

        // After processing any web view input actions, restart the frame loop.
        // The touch→click synthesis pipeline requires multiple event loop
        // spins to complete.
        if handled_input {
            self.needs_paint = true;
            self.idle_frames = 0;
            self.next_frame = cx.new_next_frame();
            cx.redraw_all();
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        crate::makepad_widgets::script_mod(vm);
        havi_render::shaders::script_mod(vm);
        crate::servo_web_view::script_mod(vm);
        crate::register_script_modules(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if let Event::Shutdown = event {
            crate::app::runtime::remove_state_file();
            self.servo = None;
            return;
        }

        // Lazy init servo on first event
        self.init_servo(cx);

        // Drain media-thread operations and forward platform video events.
        self.drain_video_ops(cx);
        self.handle_video_event(cx, event);
        if !self.audio_outputs_initialized {
            if let Event::AudioDevices(devices) = event {
                let outputs = devices.default_output();
                if !outputs.is_empty() {
                    cx.use_audio_outputs(&outputs);
                    self.audio_outputs_initialized = true;
                }
            }
        }

        if let Event::AppOpen(items) = event {
            for url in items {
                if let Some(webview) = self.create_webview(url) {
                    let webview_id = webview.id();
                    self.tabs.push(TabInfo {
                        webview_id,
                        root_pipeline_id: None,
                        webview,
                        title: title_from_url(url),
                        url: url.clone(),
                        widget_id: next_tab_live_id(),
                        watch: Default::default(),
                    });
                    self.active_tab_idx = self.tabs.len() - 1;
                    self.attach_active_render_state(cx);
                    self.activate_tab_webview(self.active_tab_idx);
                    self.focus_active_webview(cx);
                    #[cfg(any(target_os = "android", target_os = "ios"))]
                    {
                        self.pending_clipboard_menu = None;
                        self.selection_handles_visible = false;
                        cx.hide_clipboard_actions();
                        cx.hide_selection_handles();
                    }
                    self.ui.text_input(cx, ids!(url_input)).set_text(cx, url);
                    self.needs_paint = true;
                    self.sync_tab_bar(cx);
                    self.idle_frames = 0;
                    self.next_frame = cx.new_next_frame();
                }
            }
        }

        // Poll pylon background init result.
        if let Some(ref rx) = self.pylon_init_rx {
            let poll_result = match rx.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // pylon-init thread dropped sender without sending a result
                    // (panic, abort, or logic error).
                    eprintln!(
                        "[havi] pylon-init thread exited without sending a result (likely panicked)"
                    );
                    Some(PylonInitResult::Failed {
                        reason: "pylon: off (init thread crashed)".to_string(),
                    })
                },
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = poll_result {
                self.pylon_init_rx = None;
                match result {
                    PylonInitResult::Ready {
                        hpprd_port,
                        pylon_port,
                        pylon_events,
                    } => {
                        self.startup_state = StartupState::Ready;
                        log!(
                            "[havishell] pylon ready: pylon_port={} hpprd_port={}",
                            pylon_port,
                            hpprd_port
                        );
                        self.watch_fallback_endpoint = format!("127.0.0.1:{}", hpprd_port);
                        if let Some(pool) = &mut self.watch_pool {
                            pool.set_endpoint(self.watch_fallback_endpoint.clone());
                        }
                        self.pylon_events = Some(pylon_events);
                        // Create command client for interactive pylon commands.
                        if let Ok(cmd_client) =
                            havi_protocols::pylon::PylonClient::connect(pylon_port)
                        {
                            self.pylon_command_client = Some(cmd_client);
                        }
                        self.refresh_pylon_status(cx);
                        self.complete_startup_navigation(cx);

                        let pylon_bind = format!("127.0.0.1:{}", pylon_port);
                        println!("PYLON_BIND={}", pylon_bind);
                        let mut state = crate::app::runtime::included_state_entries();
                        state.push(("PYLON_BIND".to_string(), pylon_bind));
                        if let Some(bind) = crate::app::delegate::get_devtools_bind() {
                            state.push(("HAVI_DEVTOOLS".to_string(), bind));
                        }
                        crate::app::runtime::write_state_file(&state);
                    },
                    PylonInitResult::Failed { reason } => {
                        self.startup_state = StartupState::Failed;
                        log!("[havishell] pylon failed: {}", reason);
                        self.pylon_status.health = pylon_menu::PylonHealth::Red;
                        self.update_pylon_dot(cx);
                        self.complete_startup_navigation(cx);

                        let mut state = crate::app::runtime::included_state_entries();
                        if let Some(bind) = crate::app::delegate::get_devtools_bind() {
                            state.push(("HAVI_DEVTOOLS".to_string(), bind));
                        }
                        crate::app::runtime::write_state_file(&state);
                    },
                }
                self.needs_paint = true;
                self.idle_frames = 0;
                self.next_frame = cx.new_next_frame();
                cx.redraw_all();
            }
        }

        // Handle splash screen timeout (3s max).
        if self.splash_timeout.is_event(event).is_some() {
            self.splash_timeout = Timer::empty();
            if !self.start_navigation_done {
                if self.startup_state == StartupState::Booting {
                    self.startup_state = StartupState::Failed;
                    eprintln!("[havi] splash timeout: pylon did not finish in 3s, proceeding");
                }
                self.complete_startup_navigation(cx);
                self.needs_paint = true;
                self.idle_frames = 0;
                self.next_frame = cx.new_next_frame();
                cx.redraw_all();
            }
        }

        // Handle next-frame for servo update loop
        if let Some(_ne) = self.next_frame.is_event(event) {
            self.update_screenshot_mode(cx);
            self.handle_screenshot_capture_results(cx);
            for result in cx.drain_capture_results() {
                if let Some((_webview_id, request_id)) =
                    self.pending_screenshot_callbacks.remove(&result.request_id)
                {
                    if let Some(image) =
                        servo::RgbaImage::from_raw(result.width, result.height, result.rgba)
                    {
                        if let Some(servo) = &self.servo {
                            servo.paint_screenshot_bridge().push_result(request_id, image);
                        }
                    }
                }
            }
            // Drain pylon events and update status dot
            {
                let mut status_changed = false;
                if let Some(ref rx) = self.pylon_events {
                    while let Ok(ev) = rx.try_recv() {
                        match (ev.event.as_str(), ev.service.as_deref()) {
                            ("service_started", Some(svc)) => {
                                self.pylon_status
                                    .apply_event(svc, "running", ev.pid, ev.port);
                                status_changed = true;
                            },
                            ("service_stopped", Some(svc)) => {
                                self.pylon_status.apply_event(svc, "stopped", None, None);
                                status_changed = true;
                            },
                            _ => {},
                        }

                        if ev.event == "listener" && ev.service.as_deref() == Some("hpprd") {
                            if let Some(via) = ev.public_via.as_ref() {
                                self.shared_public_via = Some(via.clone());
                            } else if ev.present == Some(false)
                                || ev.source.as_deref() == Some("nat")
                            {
                                self.shared_public_via = None;
                            }
                        }
                    }
                }
                if status_changed {
                    self.update_pylon_dot(cx);
                }
            }

            // Poll HPPR watch events for active tab
            let watch_action = if let Some(pool) = &mut self.watch_pool {
                if let Some(tab) = self.tabs.get_mut(self.active_tab_idx) {
                    tab.watch.reconcile(&tab.url, pool);
                    tab.watch.poll()
                } else {
                    havi_protocols::watch::WatchAction::None
                }
            } else {
                havi_protocols::watch::WatchAction::None
            };
            match watch_action {
                havi_protocols::watch::WatchAction::Reload => {
                    self.recreate_active_tab_webview(cx);
                    self.needs_paint = true;
                },
                havi_protocols::watch::WatchAction::ChangeDetected => {
                    cx.redraw_all();
                },
                havi_protocols::watch::WatchAction::None => {},
            }

            self.update_servo_and_texture(cx);

            // Update primary selection (Linux middle-click paste) when text changes.
            #[cfg(target_os = "linux")]
            if let Some(tab) = self.tabs.get(self.active_tab_idx) {
                let snapshot = layout_api::shared_document_selection_for(tab.webview_id).snapshot();
                if snapshot.text != self.last_primary_selection {
                    if !snapshot.text.is_empty() {
                        cx.set_primary_selection(&snapshot.text);
                    }
                    self.last_primary_selection = snapshot.text;
                }
            }

            // Update selection handles and deferred clipboard menu on mobile.
            #[cfg(any(target_os = "android", target_os = "ios"))]
            if let Some(tab) = self.tabs.get(self.active_tab_idx) {
                let snapshot = layout_api::shared_document_selection_for(tab.webview_id).snapshot();

                if let (Some(first), Some(last)) = (snapshot.rects.first(), snapshot.rects.last()) {
                    let start = dvec2(
                        first.origin.x as f64,
                        (first.origin.y + first.size.height) as f64,
                    );
                    let end = dvec2(
                        (last.origin.x + last.size.width) as f64,
                        (last.origin.y + last.size.height) as f64,
                    );
                    if !self.selection_handles_visible {
                        cx.show_selection_handles(start, end);
                        self.selection_handles_visible = true;
                    } else {
                        cx.update_selection_handles(start, end);
                    }
                } else if self.selection_handles_visible {
                    cx.hide_selection_handles();
                    self.selection_handles_visible = false;
                }

                if let Some(pending) = self.pending_clipboard_menu {
                    let web_rect = self.ui.servo_web_view(cx, ids!(web_view)).area().rect(cx);
                    let local = dvec2(
                        pending.anchor_abs.x - web_rect.pos.x,
                        pending.anchor_abs.y - web_rect.pos.y,
                    );
                    let contains_anchor = snapshot.rects.iter().any(|r| {
                        let x0 = r.origin.x as f64;
                        let y0 = r.origin.y as f64;
                        let x1 = x0 + r.size.width as f64;
                        let y1 = y0 + r.size.height as f64;
                        local.x >= x0 && local.x <= x1 && local.y >= y0 && local.y <= y1
                    });
                    let ready = !snapshot.rects.is_empty()
                        && (snapshot.revision != pending.baseline_revision || contains_anchor);
                    if ready {
                        let capabilities =
                            self.selection_capabilities_for_active_tab(&snapshot, self.ime_visible);
                        let rect = makepad_widgets::Rect {
                            pos: pending.anchor_abs,
                            size: dvec2(1.0, 1.0),
                        };
                        cx.show_clipboard_actions(capabilities.can_copy, rect, 0.0);
                        ::log::trace!(
                            "[havishell] mobile clipboard actions rev={} rects={} caps={}",
                            snapshot.revision,
                            snapshot.rects.len(),
                            capabilities.summary()
                        );
                        self.pending_clipboard_menu = None;
                    }
                }
            }

            // Tick scroll fade animation
            let scroll_fading = self
                .ui
                .servo_web_view(cx, ids!(web_view))
                .tick_scroll_fade(cx, 1.0 / 60.0);

            // Continue the frame loop while there's recent activity.
            // When idle, stop to save CPU/GPU. The Wake action will restart it.
            // Keep running in control mode so stdin messages are polled.
            if self.idle_frames < MAX_IDLE_FRAMES || scroll_fading || Cx::has_studio_web_socket() {
                self.next_frame = cx.new_next_frame();
                cx.redraw_all();
            }
        }

        // When the window geometry changes (resize, DPI change), restart the
        // frame loop so check_resize() picks up the new dimensions and
        // propagates them to the Servo webview.
        if let Event::WindowGeomChange(re) = event {
            self.needs_paint = true;
            self.idle_frames = 0;
            self.next_frame = cx.new_next_frame();
            self.apply_menu_dock(cx);
            self.sync_tab_bar(cx);
            cx.redraw_all();
            // Update DPI factor if it changed (e.g., moved to different-DPI monitor)
            if re.new_geom.dpi_factor > 0.0 && re.new_geom.dpi_factor != self.dpi_factor {
                self.dpi_factor = re.new_geom.dpi_factor;
                // Update Servo webview hidpi scale factors
                let new_hidpi: Scale<f32, DeviceIndependentPixel, DevicePixel> =
                    Scale::new(self.dpi_factor as f32);
                for tab in &self.tabs {
                    tab.webview.set_hidpi_scale_factor(new_hidpi);
                }
            }
        }

        // Handle window dragging from the tab bar area (replaces hidden caption_bar).
        // Only treat empty space as caption — exclude tabs, buttons, and controls.
        if let Event::WindowDragQuery(dq) = event {
            if dq.window_id == CxWindowPool::id_zero() {
                let wrap_rect = self.ui.view(cx, ids!(tab_bar_wrap)).area().rect(cx);
                if dq.abs.y < wrap_rect.pos.y + wrap_rect.size.y {
                    let mut over_interactive = false;
                    // Check window controls
                    let controls_rect = self.ui.view(cx, ids!(window_controls)).area().rect(cx);
                    if controls_rect.contains(dvec2(dq.abs.x, dq.abs.y)) {
                        over_interactive = true;
                    }
                    // Check new tab button
                    let btn_rect = self.ui.button(cx, ids!(new_tab_btn)).area().rect(cx);
                    if btn_rect.contains(dvec2(dq.abs.x, dq.abs.y)) {
                        over_interactive = true;
                    }
                    // Check each tab
                    if let Some(tab_bar) = self.ui.view(cx, ids!(tab_bar)).borrow() {
                        for (_child_id, child_widget) in tab_bar.children.iter() {
                            let r = child_widget.area().rect(cx);
                            if r.contains(dvec2(dq.abs.x, dq.abs.y)) {
                                over_interactive = true;
                                break;
                            }
                        }
                    }
                    if !over_interactive {
                        dq.response.set(WindowDragQueryResponse::Caption);
                        cx.set_cursor(MouseCursor::Default);
                    }
                }
            }
        }

        // Handle popup window dismissal (compositor/focus-loss/outside-click/Escape).
        if let Event::PopupDismissed(ev) = event {
            self.handle_popup_dismissed(cx, ev);
        }

        // Draw context menu popup contents during draw events.
        if let Event::Draw(draw_event) = event {
            if self.context_popup_pass.is_some() {
                let mut cx_draw = CxDraw::new(cx, draw_event);
                let cx2d = &mut Cx2d::new(&mut cx_draw);
                self.draw_context_menu_popup(cx2d);
            }
        }

        // Let the widget tree handle events. The ServoWebView widget calls
        // event.hits() internally and emits ServoWebViewAction for all
        // interactions. Toolbar buttons and TextInput process their
        // interactions here as well.
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
