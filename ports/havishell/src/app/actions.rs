use super::*;

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

impl App {
    fn complete_startup_navigation(&mut self, cx: &mut Cx) {
        if self.start_navigation_done {
            return;
        }
        self.start_navigation_done = true;
        self.scroll_y_estimate = 0.0;
        self.content_height_estimate = 0.0;

        // Create the first webview if none exists yet (splash screen path).
        if self.tabs.is_empty() {
            if let Some(webview) = self.create_webview(&self.start_url) {
                let webview_id = webview.id();
                self.tabs.push(TabInfo {
                    webview_id,
                    webview,
                    title: title_from_url(&self.start_url),
                    url: self.start_url.clone(),
                    widget_id: next_tab_live_id(),
                    watch: Default::default(),
                });
                self.active_tab_idx = 0;
                self.activate_tab_webview(0);
            }
        } else {
            self.navigate(&self.start_url);
            if let Some(tab) = self.tabs.get_mut(self.active_tab_idx) {
                tab.url = self.start_url.clone();
                tab.title = title_from_url(&self.start_url);
            }
        }

        // Hide splash screen, show chrome.
        self.ui.view(cx, ids!(splash_screen)).set_visible(cx, false);
        self.ui
            .text_input(cx, ids!(url_input))
            .set_text(cx, &self.start_url);
        self.sync_tab_bar(cx);
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
                let next = tab.watch.mode().next();
                tab.watch.set_mode(next);
                if next != havi_protocols::watch::WatchMode::Off {
                    self.ensure_watch_pool();
                }
                self.ui
                    .button(cx, ids!(watch_btn))
                    .set_text(cx, &watch_button_text(next));
            }
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
            let is_fs = cx.windows[CxWindowPool::id_zero()].window_geom.is_fullscreen;
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
        if self.ui.button(cx, ids!(context_copy_btn)).clicked(actions) {
            self.hide_context_menu(cx);
            self.send_copy_command();
        }
        if self.ui.button(cx, ids!(context_edit_btn)).clicked(actions) {
            self.hide_context_menu(cx);
            let url_text = self.ui.text_input(cx, ids!(url_input)).text();
            if let Some(edit_url) = context_menu::editor_url_for(&url_text) {
                nav_action = Some(NavCommand::Navigate(edit_url));
            }
        }

        // --- Pylon dot click ---
        if self.ui.view(cx, ids!(pylon_dot)).finger_down(actions).is_some() {
            if self.pylon_menu_open {
                self.hide_pylon_menu(cx);
            } else {
                // Refresh status before showing.
                self.refresh_pylon_status(cx);
                self.show_pylon_menu(cx);
            }
        }

        // --- Pylon menu buttons ---
        if self.ui.button(cx, ids!(pylon_hpprd_start_btn)).clicked(actions) {
            self.hide_pylon_menu(cx);
            self.pylon_command(cx, "start", Some("hpprd"), None);
        }
        if self.ui.button(cx, ids!(pylon_hpprd_stop_btn)).clicked(actions) {
            self.hide_pylon_menu(cx);
            self.pylon_command(cx, "stop", Some("hpprd"), None);
        }
        if self.ui.button(cx, ids!(pylon_nfs_start_btn)).clicked(actions) {
            self.hide_pylon_menu(cx);
            self.pylon_command(cx, "start", Some("hppr-nfs"), None);
        }
        if self.ui.button(cx, ids!(pylon_nfs_stop_btn)).clicked(actions) {
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
        if self.ui.button(cx, ids!(pylon_services_btn)).clicked(actions) {
            self.hide_pylon_menu(cx);
            nav_action = Some(NavCommand::Navigate("havi:///services".into()));
        }
        if self.ui.button(cx, ids!(pylon_shutdown_btn)).clicked(actions) {
            self.hide_pylon_menu(cx);
            self.pylon_command(cx, "shutdown", None, None);
            self.pylon_status.health = pylon_menu::PylonHealth::Red;
            self.pylon_command_client = None;
            self.update_pylon_dot(cx);
        }

        // --- Tab bar events ---
        if self.ui.button(cx, ids!(tab_scroll_left_btn)).clicked(actions) {
            self.scroll_tabs(cx, -1.0);
        }
        if self.ui.button(cx, ids!(tab_scroll_right_btn)).clicked(actions) {
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
                    self.scroll_y_estimate = 0.0;
                    self.content_height_estimate = 0.0;
                    self.navigate(url);
                    // Update active tab URL
                    if let Some(tab) = self.tabs.get_mut(self.active_tab_idx) {
                        tab.url = url.clone();
                    }
                },
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
                        }
                    }
                },
                Some(MakepadServoAction::NewFrameReady { webview_id }) => {
                    let webview_id = *webview_id;
                    // Only repaint if the active webview has new content
                    if self
                        .tabs
                        .get(self.active_tab_idx)
                        .map_or(false, |t| t.webview_id == webview_id)
                    {
                        self.needs_paint = true;
                        self.idle_frames = 0;
                        self.next_frame = cx.new_next_frame();
                        cx.redraw_all();
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
                        cx.hide_text_ime();
                    }
                },
                Some(MakepadServoAction::WatchGetMode {
                    webview_id,
                    response_sender,
                }) => {
                    let mode = self
                        .tab_index_for_webview(*webview_id)
                        .and_then(|idx| self.tabs.get(idx))
                        .map(|tab| mode_to_wire(tab.watch.mode()))
                        .unwrap_or_else(|| "off".to_string());
                    let _ = response_sender.send(mode);
                },
                Some(MakepadServoAction::WatchSetMode {
                    webview_id,
                    mode,
                    response_sender,
                }) => {
                    let tab_idx = self.tab_index_for_webview(*webview_id);
                    let mut need_watch_pool = false;
                    let new_mode = if let Some(mode) = mode_from_wire(mode) {
                        if mode != havi_protocols::watch::WatchMode::Off {
                            need_watch_pool = true;
                        }
                        if let Some(idx) = tab_idx {
                            if let Some(tab) = self.tabs.get_mut(idx) {
                                tab.watch.set_mode(mode);
                                if idx == self.active_tab_idx {
                                    self.ui
                                        .button(cx, ids!(watch_btn))
                                        .set_text(cx, &watch_button_text(mode));
                                }
                                mode_to_wire(tab.watch.mode())
                            } else {
                                "off".to_string()
                            }
                        } else {
                            "off".to_string()
                        }
                    } else {
                        tab_idx
                            .and_then(|idx| self.tabs.get(idx))
                            .map(|tab| mode_to_wire(tab.watch.mode()))
                            .unwrap_or_else(|| "off".to_string())
                    };
                    if need_watch_pool {
                        self.ensure_watch_pool();
                    }
                    let _ = response_sender.send(new_mode);
                },
                _ => {},
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
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        // Lazy init servo on first event
        self.init_servo(cx);

        // Handle IPC commands (single-instance tab open requests)
        {
            let mut ipc_urls = Vec::new();
            if let Some(ref rx) = self.ipc_rx {
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        havi_protocols::instance::IpcCommand::Open { url } => {
                            ipc_urls.push(url);
                        }
                    }
                }
            }
            for url in ipc_urls {
                if let Some(webview) = self.create_webview(&url) {
                    let webview_id = webview.id();
                    self.tabs.push(TabInfo {
                        webview_id,
                        webview,
                        title: title_from_url(&url),
                        url: url.clone(),
                        widget_id: next_tab_live_id(),
                        watch: Default::default(),
                    });
                    self.active_tab_idx = self.tabs.len() - 1;
                    self.activate_tab_webview(self.active_tab_idx);
                    self.ui.text_input(cx, ids!(url_input)).set_text(cx, &url);
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
                    eprintln!("[havi] pylon-init thread exited without sending a result (likely panicked)");
                    Some(PylonInitResult::Failed {
                        reason: "pylon: off (init thread crashed)".to_string(),
                    })
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = poll_result {
                self.pylon_init_rx = None;
                match result {
                    PylonInitResult::Ready { hpprd_port, pylon_port, pylon_events } => {
                        self.startup_state = StartupState::Ready;
                        log!("[havishell] pylon ready: pylon_port={} hpprd_port={}", pylon_port, hpprd_port);
                        self.watch_fallback_endpoint = format!("127.0.0.1:{}", hpprd_port);
                        if let Some(pool) = &mut self.watch_pool {
                            pool.set_endpoint(self.watch_fallback_endpoint.clone());
                        }
                        self.pylon_events = Some(pylon_events);
                        // Create command client for interactive pylon commands.
                        if let Ok(cmd_client) = havi_protocols::pylon::PylonClient::connect(pylon_port) {
                            self.pylon_command_client = Some(cmd_client);
                        }
                        self.refresh_pylon_status(cx);
                        self.complete_startup_navigation(cx);
                    }
                    PylonInitResult::Failed { reason } => {
                        self.startup_state = StartupState::Failed;
                        log!("[havishell] pylon failed: {}", reason);
                        self.pylon_status.health = pylon_menu::PylonHealth::Red;
                        self.update_pylon_dot(cx);
                        self.complete_startup_navigation(cx);
                    }
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
            // Drain pylon events and update status dot
            {
                let mut status_changed = false;
                if let Some(ref rx) = self.pylon_events {
                    while let Ok(ev) = rx.try_recv() {
                        match (ev.event.as_str(), ev.service.as_deref()) {
                            ("service_started", Some(svc)) => {
                                self.pylon_status.apply_event(svc, "running", ev.pid, ev.port);
                                status_changed = true;
                            }
                            ("service_stopped", Some(svc)) => {
                                self.pylon_status.apply_event(svc, "stopped", None, None);
                                status_changed = true;
                            }
                            _ => {}
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
                    self.reload();
                    self.needs_paint = true;
                }
                havi_protocols::watch::WatchAction::ChangeDetected => {
                    cx.redraw_all();
                }
                havi_protocols::watch::WatchAction::None => {}
            }

            self.update_servo_and_texture(cx);

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

        // Let the widget tree handle events. The ServoWebView widget calls
        // event.hits() internally and emits ServoWebViewAction for all
        // interactions. Toolbar buttons and TextInput process their
        // interactions here as well.
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
