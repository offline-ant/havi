use super::*;

/// GL_TEXTURE_RECTANGLE constant (macOS CGL/IOSurface textures).
const GL_TEXTURE_RECTANGLE: u32 = 0x84F5;

/// Build platform display info for WebGL from the GL render bridge.
#[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
fn build_display_info(
    bridge: &makepad_widgets::makepad_platform::gl_render_bridge::GlRenderBridge,
) -> servo::gl_device::egl::EglDisplayInfo {
    let egl_gpa: unsafe extern "C" fn(*const std::ffi::c_char) -> *mut std::ffi::c_void = unsafe {
        std::mem::transmute(bridge.get_proc_address("eglGetProcAddress"))
    };
    servo::gl_device::egl::EglDisplayInfo {
        display: bridge.egl_display(),
        config: bridge.egl_config(),
        share_context: bridge.egl_context(),
        get_proc_address: egl_gpa,
    }
}

#[cfg(target_os = "macos")]
fn build_display_info(
    bridge: &makepad_widgets::makepad_platform::gl_render_bridge::GlRenderBridge,
) -> servo::gl_device::cgl::CglDisplayInfo {
    servo::gl_device::cgl::CglDisplayInfo {
        pixel_format: bridge.cgl_pixel_format(),
        share_context: bridge.cgl_context(),
    }
}

#[cfg(target_os = "ios")]
fn build_display_info(
    bridge: &makepad_widgets::makepad_platform::gl_render_bridge::GlRenderBridge,
) -> servo::gl_device::eagl::EaglDisplayInfo {
    servo::gl_device::eagl::EaglDisplayInfo {
        share_context: bridge.eagl_context(),
        opengles_framework: bridge.opengles_framework(),
    }
}

/// Create a rendering context for Servo's internal pipeline.
/// With direct Makepad rendering, we don't use the GL output, but Servo
/// still requires a RenderingContext for pipeline creation.
fn create_rendering_context(
    cx: &mut Cx,
    size: dpi::PhysicalSize<u32>,
) -> Result<Rc<servo::MakepadRenderingContext>, servo::rendering_context::Error> {
    let bridge = cx.create_gl_render_bridge();
    bridge.make_current();

    let display_info = Some(build_display_info(&bridge));

    let gl_api = match bridge.gl_api() {
        GlApi::GL => servo::gl_device::GlApi::GL,
        GlApi::GLES => servo::gl_device::GlApi::GLES,
    };
    let texture_target = match bridge.gl_api() {
        GlApi::GL => GL_TEXTURE_RECTANGLE,
        GlApi::GLES => 0x0DE1, // GL_TEXTURE_2D
    };

    let rc = unsafe {
        servo::MakepadRenderingContext::new_from_loader(
            size,
            &|name| bridge.get_proc_address(name) as *const std::ffi::c_void,
            gl_api,
            texture_target,
            display_info,
        )
    }?;
    cx.restore_gl_context();

    Ok(Rc::new(rc))
}

impl App {
    pub(super) fn init_servo(&mut self, cx: &mut Cx) {
        if self.initialized {
            return;
        }

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

        self.initialized = true;
        self.dpi_factor = dpi_factor;
        log!("[havishell] init_servo: dpi={} size={}x{}", dpi_factor, inner.x, inner.y);

        // Init resource reader
        servo::resources::set(Box::new(ResourceReader));

        // Use physical pixel dimensions for the initial texture.
        // Makepad's inner_size is in logical pixels; multiply by DPI for physical.
        let width = ((inner.x * self.dpi_factor) as u32).max(64);
        let height = ((inner.y * self.dpi_factor) as u32).max(64);
        self.content_size = (width as usize, height as usize);

        // Create rendering context + texture via the unified GL render bridge.
        let size = dpi::PhysicalSize::new(width, height);
        let rendering_context = match create_rendering_context(cx, size) {
            Ok(result) => result,
            Err(e) => {
                log!("[havishell] FAILED to create rendering context: {:?}", e);
                return;
            },
        };

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
        let repo_path = havi_protocols::config::repo_dir();
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
                havi_protocols::client::HpprdClientAsync::new(target)
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
        let mut preferences = servo::Preferences::default();
        preferences.set_value("viewport_meta_enabled", servo::PrefValue::Bool(true));

        // Enable devtools. HAVI_DEVTOOLS env var overrides the listen address
        // (e.g. "6080" or "127.0.0.1:6080"). In debug builds, devtools defaults
        // to port 0 (OS-assigned) so the effective port is printed at startup.
        if let Ok(devtools_addr) = std::env::var("HAVI_DEVTOOLS") {
            preferences.devtools_server_enabled = true;
            preferences.devtools_server_listen_address = devtools_addr;
        } else if cfg!(debug_assertions) {
            preferences.devtools_server_enabled = true;
            preferences.devtools_server_listen_address = "0".to_string();
        }

        let servo = servo::ServoBuilder::default()
            .event_loop_waker(Box::new(MakepadEventLoopWaker))
            .preferences(preferences)
            .protocol_registry(protocol_registry)
            .hppr_home_target(fallback_target.clone())
            .build();
        servo.set_delegate(Rc::new(HaviServoDelegate));
        servo.setup_logging();

        self.servo = Some(servo);
        self.rendering_context = Some(rendering_context);

        // Step 5: Create first WebView or show splash screen.
        if pylon_mode == PylonMode::None {
            // No pylon boot — create webview immediately, hide splash.
            let initial_url_str = self.start_url.clone();
            if let Some(webview) = self.create_webview(&initial_url_str) {
                let webview_id = webview.id();
                // Wire shared fragment tree for direct Makepad rendering.
                let shared = layout_api::shared_fragment_tree_for(webview_id);
                let scroll = layout_api::shared_scroll_state_for(webview_id);
                let images = self.servo.as_ref().unwrap().image_store();
                self.ui
                    .servo_web_view(cx, ids!(web_view))
                    .set_shared_fragments(shared, scroll, images);
                self.tabs.push(TabInfo {
                    webview_id,
                    webview,
                    title: title_from_url(&initial_url_str),
                    url: initial_url_str.clone(),
                    widget_id: next_tab_live_id(),
                    watch: Default::default(),
                });
                self.active_tab_idx = 0;
            }
            self.ui.view(cx, ids!(splash_screen)).set_visible(cx, false);
            self.ui
                .text_input(cx, ids!(url_input))
                .set_text(cx, &self.start_url);
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
        self.ui.button(cx, ids!(dock_btn)).set_text(cx, "🔼");
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

        // Start HAVI IPC listener for single-instance support
        havi_protocols::instance::set_signal_callback(|| {
            SignalToUI::set_ui_signal();
        });
        match havi_protocols::instance::start_ipc_listener() {
            Ok(rx) => self.ipc_rx = Some(rx),
            Err(e) => log!("[havishell] IPC listener: {}", e),
        }

        // Signal that we need to paint the first frame
        self.needs_paint = true;
        self.idle_frames = 0;

        // Print eval-compatible environment summary.
        // PYLON= is printed later when PylonReady arrives.
        {
            let repo_dir = havi_protocols::config::repo_dir();
            eprintln!("HPPRD_REPO={}", repo_dir.display());
            eprintln!("HAVI_URL={}", self.start_url);
            eprintln!(
                "[havi] startup: state={:?}, start_navigation_done={}",
                self.startup_state,
                self.start_navigation_done
            );
        }

        // Start the frame loop
        self.next_frame = cx.new_next_frame();

        // Control mode: stdin/stdout JSON protocol.
        // Skip when running inside Makepad Studio's RunView — stdin is already
        // used by the Studio WebSocket protocol.
        if std::env::var("HAVI_MAKEPAD_EVENTS").is_ok() && !cx.in_makepad_studio {
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
                        }
                        Err(e) => {
                            eprintln!("[havi-makepad-events] parse error: {:?} for: {}", e, line);
                        }
                    }
                }
            });

            use std::io::Write;
            let _ = std::io::stdout().write_all(b"{\"ReadyToStart\":null}\n");
            let _ = std::io::stdout().flush();
        }
    }

    /// Check if the web_view widget has been resized and update the rendering context
    /// and texture accordingly.
    fn check_resize(&mut self, cx: &mut Cx) {
        let rect = self.ui.servo_web_view(cx, ids!(web_view)).area().rect(cx);
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

        ::log::info!(
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

        // Signal that we need to redraw at the new size.
        self.needs_paint = true;
    }

    /// Main update method called each frame. Spins Servo's event loop and
    /// optionally does the expensive paint + readback cycle.
    pub(super) fn update_servo_and_texture(&mut self, cx: &mut Cx) {
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
                self.ui.text_input(cx, ids!(url_input)).set_text(cx, &url);
                self.sync_tab_bar(cx);
            }
        }

        // Check for widget resize
        self.check_resize(cx);

        // Reset idle counter when new content is available.
        if self.needs_paint {
            self.needs_paint = false;
            self.idle_frames = 0;
        } else {
            self.idle_frames = self.idle_frames.saturating_add(1);
        }
    }

    pub(super) fn point_to_device(&self, cx: &mut Cx, pos: DVec2) -> servo::DevicePoint {
        let rect = self.ui.servo_web_view(cx, ids!(web_view)).area().rect(cx);
        // pos and rect are in Makepad logical pixels; Servo wants device pixels.
        let x = ((pos.x - rect.pos.x) * self.dpi_factor) as f32;
        let y = ((pos.y - rect.pos.y) * self.dpi_factor) as f32;
        servo::DevicePoint::new(x, y)
    }

    /// Get the active tab's webview, if any.
    pub(super) fn active_webview(&self) -> Option<&servo::WebView> {
        self.tabs.get(self.active_tab_idx).map(|t| &t.webview)
    }

    pub(super) fn send_input_event(&self, event: servo::InputEvent) {
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
        self.watch_pool = Some(havi_protocols::watch::WatchPool::new(
            handle,
            self.watch_fallback_endpoint.clone(),
            SignalToUI::set_ui_signal,
        ));
    }


}
