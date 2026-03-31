use super::*;

pub(super) fn included_state_entries() -> Vec<(String, String)> {
    let Ok(include) = std::env::var("HAVI_INCLUDE_STATE") else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for key in include.split(',') {
        if key.is_empty() {
            continue;
        }
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() {
                out.push((key.to_string(), value));
            }
        }
    }
    out
}

pub(super) fn write_state_file(lines: &[(String, String)]) {
    let Some(socket_path) = makepad_widgets::makepad_platform::single_instance::app_socket_path() else {
        return;
    };
    let state_path = std::path::PathBuf::from(format!("{}.state", socket_path.display()));
    let _ = std::fs::remove_file(&state_path);
    if let Some(parent) = state_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut out = String::new();
    for (key, value) in lines {
        out.push_str(key);
        out.push('=');
        out.push_str(value);
        out.push('\n');
    }
    let _ = std::fs::write(&state_path, out);
}

pub(super) fn remove_state_file() {
    let Some(socket_path) = makepad_widgets::makepad_platform::single_instance::app_socket_path() else {
        return;
    };
    let state_path = std::path::PathBuf::from(format!("{}.state", socket_path.display()));
    let _ = std::fs::remove_file(state_path);
}

impl App {
    pub(super) fn init_servo(&mut self, cx: &mut Cx) {
        if self.initialized {
            return;
        }

        remove_state_file();

        // Wait until the window geometry is populated by the platform layer.
        // On Android, dpi_factor and inner_size are set asynchronously:
        // dpi_factor comes from FromJavaMessage::Init, inner_size from SurfaceChanged.
        // Both must be valid before we can create correctly-sized textures.
        let geom = &cx.windows[CxWindowPool::id_zero()].window_geom;
        let dpi_factor = geom.dpi_factor;
        let inner = geom.inner_size;
        if dpi_factor <= 0.0 || inner.x <= 0.0 || inner.y <= 0.0 {
            return;
        }

        // Set Wayland app_id to "havi"
        cx.windows[CxWindowPool::id_zero()].create_app_id = "havi".to_string();

        self.init_media_bridge(cx);

        self.initialized = true;
        self.dpi_factor = dpi_factor;
        log!(
            "[havishell] init_servo: dpi={} size={}x{}",
            dpi_factor,
            inner.x,
            inner.y
        );

        // Init resource reader
        libhavi::resources::set(Box::new(ResourceReader));

        // Use physical pixel dimensions for the initial texture.
        // Makepad's inner_size is in logical pixels; multiply by DPI for physical.
        let width = ((inner.x * self.dpi_factor) as u32).max(64);
        let height = ((inner.y * self.dpi_factor) as u32).max(64);
        self.content_size = (width as usize, height as usize);

        #[cfg(target_os = "android")]
        {
            if std::env::var_os("HAVI_CONFIG").is_none() {
                if let Some(data_dir) = cx.get_data_dir() {
                    let config_dir = std::path::Path::new(&data_dir).join("HAVI");
                    if let Err(err) = std::fs::create_dir_all(&config_dir) {
                        eprintln!(
                            "[havi] failed to create android HAVI_CONFIG at {}: {}",
                            config_dir.display(),
                            err
                        );
                    } else {
                        std::env::set_var("HAVI_CONFIG", &config_dir);
                        log!("[havishell] HAVI_CONFIG={}", config_dir.display());
                    }
                }
            }
        }

        let home = std::env::var("HAVI_HOME").ok().filter(|v| !v.is_empty());
        let repo_path = libhavi::hppr::config::repo_dir();
        let fallback_target = home
            .as_deref()
            .and_then(|v| hppr_client::parse_via(v).ok())
            .unwrap_or(hppr_client::ViaSpec::Net {
                host: "127.0.0.1".to_string(),
                port: hppr_client::DEFAULT_PORT,
                scheme: Some(hppr_client::TransportScheme::Tcp),
            });

        // Fallback target used locally until pylon/hpprd startup resolves.

        let pylon_mode = pylon_mode_from_env();
        self.start_url = std::env::var("HAVI_URL").unwrap_or_else(|_| "havi:///".to_string());
        self.screenshot_mode = std::env::var("HAVI_SCREENSHOT")
            .ok()
            .filter(|path| !path.is_empty())
            .map(|path| super::screenshot::ScreenshotMode::WaitingForLoad {
                output_path: path.into(),
                deadline: std::time::Instant::now()
                    + std::time::Duration::from_millis(super::screenshot::SCREENSHOT_MAX_LOAD_WAIT_MS),
            });
        self.screenshot_poll = Timer::empty();
        self.start_navigation_done = pylon_mode == PylonMode::None;

        // Startup state machine: Booting -> Ready/Failed.
        self.startup_state = if pylon_mode == PylonMode::None {
            StartupState::Ready
        } else {
            StartupState::Booting
        };

        // Spawn pylon + hpprd + credential bootstrap on a background thread.
        if pylon_mode != PylonMode::None {
            let home_clone = home.clone();
            let repo_path_clone = repo_path.clone();
            let (pylon_tx, pylon_rx) = std::sync::mpsc::channel();
            self.pylon_init_rx = Some(pylon_rx);
            std::thread::Builder::new()
                .name("pylon-init".to_string())
                .spawn(move || {
                    let msg = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let host_mode = match pylon_mode {
                            PylonMode::External => crate::pylon_host::PylonHostMode::External,
                            PylonMode::Embedded => crate::pylon_host::PylonHostMode::Embedded,
                            PylonMode::None => unreachable!(),
                        };

                        eprintln!("[havi] pylon-init: mode={:?}, repo={}", host_mode, repo_path_clone.display());

                        let mut pylon_client = crate::pylon_host::ensure_pylon(
                            &repo_path_clone,
                            home_clone.as_deref(),
                            host_mode,
                        ).map_err(|e| {
                            eprintln!("[havi] pylon unavailable: {:#}", e);
                            format!("pylon: off ({})", e)
                        })?;

                        eprintln!("[havi] pylon-init: connected to pylon on port {}", pylon_client.port);

                        let hpprd_runtime = match pylon_mode {
                            PylonMode::Embedded => Some("inline"),
                            PylonMode::External | PylonMode::None => None,
                        };

                        let hpprd_port = start_hpprd_with_runtime(&mut pylon_client, hpprd_runtime)
                            .map_err(|e| {
                                eprintln!("[havi] hpprd start failed: {:#}", e);
                                "pylon: off (hpprd start failed)".to_string()
                            })?;

                        eprintln!("[havi] pylon-init: hpprd on port {}", hpprd_port);

                        let pylon_port = pylon_client.port;
                        let events = pylon_client.subscribe();

                        // Bootstrap credentials now that hpprd is reachable.
                        let target = hppr_client::ViaSpec::Net {
                            host: "127.0.0.1".to_string(),
                            port: hpprd_port,
                            scheme: Some(hppr_client::TransportScheme::Tcp),
                        };
                        let credential_store = global_credential_store();
                        let endpoint = hppr_client::repo_endpoint_from(&target);
                        match hppr_client::connect_tcp(&endpoint, hppr_client::Signer::anyone()) {
                            Ok(mut client) => {
                                if let Ok(greeting) = client.hello() {
                                    let key = greeting.verifying_key();
                                    if credential_store.load_admin_for_key(key).is_err() {
                                        credential_store.bootstrap_admin();
                                        let _ = credential_store.persist_admin_for_key(key);
                                    }
                                } else {
                                    eprintln!("[havi] pylon-init: hpprd hello failed, bootstrapping admin");
                                    credential_store.bootstrap_admin();
                                }
                            },
                            Err(e) => {
                                eprintln!("[havi] pylon-init: hpprd connect failed ({}), bootstrapping admin", e);
                                credential_store.bootstrap_admin();
                            },
                        }

                        Ok::<_, String>((hpprd_port, pylon_port, events))
                    })) {
                        Ok(Ok((hpprd_port, pylon_port, pylon_events))) => {
                            PylonInitResult::Ready { hpprd_port, pylon_port, pylon_events }
                        }
                        Ok(Err(reason)) => {
                            PylonInitResult::Failed { reason }
                        }
                        Err(panic_payload) => {
                            let panic_msg = panic_payload
                                .downcast_ref::<String>()
                                .map(|s| s.as_str())
                                .or_else(|| panic_payload.downcast_ref::<&str>().copied())
                                .unwrap_or("unknown panic");
                            eprintln!("[havi] pylon-init thread panicked: {}", panic_msg);
                            PylonInitResult::Failed {
                                reason: format!("pylon: off (panic: {})", panic_msg),
                            }
                        }
                    };
                    let _ = pylon_tx.send(msg);
                    SignalToUI::set_ui_signal();
                })
                .expect("failed to spawn pylon-init thread");
        }

        // Watch runtime/pool are created lazily on first watch usage.
        self.watch_fallback_endpoint = hppr_client::repo_endpoint_from(&fallback_target);
        self.havi_runtime = None;
        self.watch_pool = None;

        // Initialize HPPR protocol handlers with the local fallback target.
        let hppr_handler = {
            let target = fallback_target.clone();
            Arc::new(
                libhavi::hppr::client::HpprdClientAsync::new(target)
                    .expect("invalid repo endpoint"),
            )
        };
        let credential_store = global_credential_store();

        let mut protocol_registry = ProtocolRegistry::default();
        let _ = protocol_registry.register(
            "hppr",
            crate::protocols::hppr::HpprHandler::new(
                hppr_handler.clone(),
                credential_store.clone(),
            ),
        );
        let _ = protocol_registry.register(
            "havi",
            crate::protocols::havi::HaviHandler::new(
                hppr_handler.clone(),
                credential_store.clone(),
            ),
        );
        let _ = protocol_registry.register(
            "hppr-browse",
            crate::protocols::hppr_browse::HpprBrowseHandler::new(
                hppr_handler.clone(),
                credential_store.clone(),
            ),
        );
        let _ = protocol_registry.register(
            "hppr-sandbox",
            crate::protocols::hppr_sandbox::HpprSandboxHandler::new(),
        );
        let _ = protocol_registry.register(
            "hppr-setup",
            crate::protocols::hppr_setup::HpprSetupHandler::new(
                hppr_handler.clone(),
                credential_store.clone(),
            ),
        );
        let _ = protocol_registry.register(
            "hppr-join",
            crate::protocols::hppr_join::HpprJoinHandler::new(
                hppr_handler.clone(),
                credential_store.clone(),
            ),
        );
        let _ = protocol_registry.register(
            "hppr-editor",
            crate::protocols::hppr_editor::HpprEditorHandler::new(
                hppr_handler.clone(),
                credential_store.clone(),
            ),
        );
        let _ = protocol_registry.register(
            "file",
            crate::protocols::file::FileHpprHandler::new(
                hppr_handler.clone(),
                credential_store.clone(),
            ),
        );

        // Step 3: Create Servo instance with viewport_meta_enabled so that
        // <meta name="viewport" content="width=device-width"> tags are respected.
        let mut preferences = libhavi::Preferences::default();
        preferences.set_value("viewport_meta_enabled", libhavi::PrefValue::Bool(true));

        // Enable devtools only when explicitly requested.
        if let Ok(devtools_addr) = std::env::var("HAVI_DEVTOOLS") {
            preferences.devtools_server_enabled = true;
            preferences.devtools_server_listen_address = devtools_addr;
        }

        self.clipboard_state = Some(ClipboardState::new());

        let servo = libhavi::ServoBuilder::default()
            .event_loop_waker(Box::new(MakepadEventLoopWaker))
            .preferences(preferences)
            .protocol_registry(protocol_registry)
            .hppr_home_target(fallback_target.clone())
            .build();
        servo.set_delegate(Rc::new(HaviServoDelegate));
        servo.setup_logging();

        self.servo = Some(servo);

        // Step 5: startup navigation.
        if pylon_mode == PylonMode::None {
            self.ui.view(cx, ids!(splash_screen)).set_visible(cx, false);

            let mut state = included_state_entries();
            if let Some(bind) = crate::app::delegate::get_devtools_bind() {
                state.push(("HAVI_DEVTOOLS".to_string(), bind));
            }
            write_state_file(&state);

            self.sync_content_size_from_host_rect(cx);
            let start_url = self.start_url.clone();
            self.open_tab(cx, &start_url);
        } else {
            // Pylon booting — show splash screen, start 3s timeout.
            self.ui.view(cx, ids!(splash_screen)).set_visible(cx, true);
            self.splash_timeout = cx.start_timeout(3.0);
        }

        // Set initial pylon dot state.
        if pylon_mode == PylonMode::None {
            self.pylon_status.health = pylon_menu::PylonHealth::Red;
        }
        self.update_pylon_dot(cx);

        // Set window title caption to "havi"
        self.ui
            .widget(cx, ids!(caption_bar.caption_label.label))
            .set_text(cx, "havi");

        // Sync tab bar UI
        self.sync_tab_bar(cx);
        self.ui
            .button(cx, ids!(dock_btn))
            .set_text(cx, dock_button_text(self.menu_at_bottom));
        self.apply_menu_dock(cx);

        // Hide the Window's built-in caption bar — we use our own tab_bar_wrap
        self.ui.view(cx, ids!(caption_bar)).set_visible(cx, false);

        // Hide macOS traffic light buttons — HAVI uses its own window controls
        cx.push_unique_platform_op(CxOsOp::HideWindowButtons(CxWindowPool::id_zero()));

        // In Makepad Studio's RunView, or on mobile targets, window control
        // buttons are meaningless.
        if cx.in_makepad_studio || cfg!(any(target_os = "android", target_os = "ios")) {
            self.ui
                .view(cx, ids!(window_controls))
                .set_visible(cx, false);
        }

        // Signal that we need to spin the first frame.
        self.needs_spin = true;
        self.idle_frames = 0;

        // Print eval-compatible environment summary.
        // PYLON= is printed later when PylonReady arrives.
        {
            let repo_dir = libhavi::hppr::config::repo_dir();
            println!("HPPRD_REPO={}", repo_dir.display());
            println!("HAVI_URL={}", self.start_url);
            eprintln!(
                "# [havi] startup: state={:?}, start_navigation_done={}",
                self.startup_state, self.start_navigation_done
            );
        }

        // Start the frame loop
        self.next_frame = cx.new_next_frame();
        if self.screenshot_mode.is_some() {
            self.request_spin_redraw(cx);
        }

        // Control mode. KEEP existing stdin/stdout behavior when
        // HAVI_MAKEPAD_EVENTS is set.
        // Skip when running inside Makepad Studio's RunView — stdin is already
        // used by the Studio WebSocket protocol.
        if !cx.in_makepad_studio && std::env::var("HAVI_MAKEPAD_EVENTS").is_ok() {
            Cx::set_studio_stdout_mode(true);
            cx.in_makepad_studio = true;

            let (tx, rx) = mpsc::channel();
            Cx::set_control_channel(rx);
            std::thread::spawn(move || {
                use std::io::BufRead;
                let stdin = std::io::stdin();
                let reader = std::io::BufReader::new(stdin.lock());
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    if line.is_empty() {
                        continue;
                    }
                    match StudioToApp::deserialize_json(&line) {
                        Ok(msg) => {
                            if tx.send(msg).is_err() {
                                break;
                            }
                            SignalToUI::set_ui_signal();
                        },
                        Err(e) => {
                            eprintln!("[havi-makepad-events] parse error: {:?} for: {}", e, line);
                        },
                    }
                }
            });

            use std::io::Write;
            let _ = std::io::stdout().write_all(b"{\"ReadyToStart\":null}\n");
            let _ = std::io::stdout().flush();
        }
    }

    fn webview_host_rect(&mut self, cx: &mut Cx) -> Rect {
        self.ui.view(cx, ids!(content_area)).area().rect(cx)
    }

    pub(super) fn sync_content_size_from_host_rect(&mut self, cx: &mut Cx) {
        let rect = self.webview_host_rect(cx);
        let new_width = ((rect.size.x * self.dpi_factor) as u32).max(1);
        let new_height = ((rect.size.y * self.dpi_factor) as u32).max(1);
        if new_width < 64 || new_height < 64 {
            return;
        }
        self.content_size = (new_width as usize, new_height as usize);
    }

    /// Check if the web_view host widget has been resized and update the rendering context
    /// and texture accordingly.
    fn check_resize(&mut self, cx: &mut Cx) {
        let rect = self.webview_host_rect(cx);
        // Makepad's rect is in logical (DPI-independent) pixels.
        // Servo and GL textures need physical pixel dimensions.
        let new_width = ((rect.size.x * self.dpi_factor) as u32).max(1);
        let new_height = ((rect.size.y * self.dpi_factor) as u32).max(1);

        let (cur_w, cur_h) = self.content_size;
        if new_width as usize == cur_w && new_height as usize == cur_h {
            return;
        }

        // Avoid very small sizes during layout transitions
        if new_width < 64 || new_height < 64 {
            return;
        }

        log!(
            "Resizing rendering context: {}x{} → {}x{}",
            cur_w,
            cur_h,
            new_width,
            new_height
        );

        self.content_size = (new_width as usize, new_height as usize);

        let phys_size = dpi::PhysicalSize::new(new_width, new_height);

        // Resize all webviews so they're ready when switched to.
        for tab in &self.tabs {
            tab.webview.resize(phys_size);
        }

        // Resizing the active render target changes visible page output.
        self.note_active_page_visual_change();
        self.needs_spin = true;
    }

    /// Main update method called each frame. Spins Servo's event loop and
    /// optionally does the expensive paint + readback cycle.
    pub(super) fn update_servo_and_texture(&mut self, cx: &mut Cx) {
        self.check_resize(cx);

        // Always spin the event loop to process Servo's internal messages.
        // This is lightweight when there's nothing to do.
        if let Some(servo) = &self.servo {
            servo.spin_event_loop();
        }

        // Poll webview state directly after spin_event_loop.
        // The delegate's Cx::post_action goes through an mpsc channel that is
        // only drained on timer-0, so title/URL updates from the delegate can
        // lag behind. Polling the webview's already-updated fields here ensures
        // the tab bar reflects changes in the same frame.
        {
            let mut tab_bar_dirty = false;
            for tab in &mut self.tabs {
                let new_title = tab
                    .webview
                    .page_title()
                    .unwrap_or_else(|| title_from_url(&tab.url));
                if new_title != tab.title {
                    tab.title = new_title;
                    tab_bar_dirty = true;
                }
                if let Some(new_url) = tab.webview.url() {
                    let new_url_str = new_url.as_str();
                    if new_url_str != tab.url {
                        tab.url = new_url_str.to_owned();
                        tab_bar_dirty = true;
                    }
                }
            }
            if tab_bar_dirty {
                // Update URL bar for active tab
                let url = self.tabs[self.active_tab_idx].url.clone();
                self.set_url_input_sanitized(cx, &url);
                self.sync_tab_bar(cx);
            }
        }

        // Reset idle counter when loop work is pending.
        if self.needs_spin {
            self.needs_spin = false;
            self.idle_frames = 0;
        } else {
            self.idle_frames = self.idle_frames.saturating_add(1);
        }
    }

    pub(super) fn point_to_device(&mut self, cx: &mut Cx, pos: DVec2) -> libhavi::DevicePoint {
        let rect = self.webview_host_rect(cx);
        // pos and rect are in Makepad logical pixels; Servo wants device pixels.
        let x = ((pos.x - rect.pos.x) * self.dpi_factor) as f32;
        let y = ((pos.y - rect.pos.y) * self.dpi_factor) as f32;
        libhavi::DevicePoint::new(x, y)
    }

    /// Get the active tab's webview, if any.
    pub(super) fn active_webview(&self) -> Option<&libhavi::WebView> {
        self.tabs.get(self.active_tab_idx).map(|t| &t.webview)
    }

    pub(super) fn send_input_event(&self, event: libhavi::InputEvent) {
        if let Some(webview) = self.active_webview() {
            webview.notify_input_event(event);
            if let Some(servo) = &self.servo {
                servo.spin_event_loop();
            }
        }
    }

    pub(super) fn ensure_watch_pool(&mut self) {
        if self.watch_pool.is_some() {
            return;
        }

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("havi-watch")
            .build()
            .expect("failed to create HAVI watch runtime");
        let handle = runtime.handle().clone();
        self.havi_runtime = Some(runtime);
        self.watch_pool = Some(libhavi::hppr::watch::WatchPool::new(
            handle,
            self.watch_fallback_endpoint.clone(),
            SignalToUI::set_ui_signal,
        ));
    }
}
