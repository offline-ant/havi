use euclid::Scale;
use makepad_widgets::*;
use servo::{
    DeviceIndependentPixel, DevicePixel,
    RenderingContext,
    WebViewId,
};
use servo::protocol_handler::ProtocolRegistry;
use havi_protocols::embedded_hpprd::{EmbeddedHpprd, HpprdMode};
use havi_protocols::credentials::global_credential_store;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use makepad_widgets::makepad_platform::studio::StudioToApp;
use makepad_widgets::makepad_platform::thread::SignalToUI;
use makepad_widgets::makepad_platform::makepad_micro_serde::DeJson;

mod context_menu;
mod input_handling;
mod navigation;
mod tabs;

use navigation::NavCommand;
use tabs::{TabInfo, HOME_URL, title_from_url, next_tab_live_id};

#[allow(unused_imports)] // ServoWebView is used inside the script_mod! macro
use crate::servo_web_view::{ServoWebView, ServoWebViewAction, ServoWebViewWidgetRefExt};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.ServoWebView

    // Define the App and its UI layout
    let app = startup() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                window.title: "havi"
                window.inner_size: vec2(1280, 800)

                pass.clear_color: vec4(0.165, 0.165, 0.165, 1.0)
                body +: {
                    flow: Down

                    // --- Tab bar ---
                    tab_bar_wrap := View{
                        flow: Right
                        width: Fill height: Fit
                        draw_bg.color: #x1e1e1e
                        show_bg: true
                        align: Align{y: 1.0}

                        tab_bar := View{
                            flow: Right
                            event_order: Down
                            width: Fill height: Fit
                            padding: Inset{left: 4 right: 0 top: 4 bottom: 0}
                            spacing: 1
                            align: Align{y: 1.0}
                            scroll_bars: ScrollBarsTabs{
                                show_scroll_x: true
                                show_scroll_y: false
                                scroll_bar_x +: {
                                    bar_size: 4.0
                                    use_vertical_finger_scroll: true
                                }
                            }

                            // Template tab — extracted at init, not displayed directly
                            tab_template := View{
                                cursor: MouseCursor.Hand
                                flow: Right
                                width: Fit height: Fit
                                padding: Inset{left: 12 right: 4 top: 6 bottom: 6}
                                spacing: 6
                                align: Align{y: 0.5}
                                show_bg: true
                                draw_bg +: {
                                    color: uniform(#x2a2a2a)
                                    border_radius: uniform(6.0)
                                    pixel: fn() {
                                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                        sdf.box(0.0 0.0 self.rect_size.x self.rect_size.y + self.border_radius self.border_radius)
                                        sdf.fill(self.color)
                                        return sdf.result
                                    }
                                }
                                tab_label := Label{
                                    text: "New Tab"
                                    draw_text.color: #xcccccc
                                    draw_text.text_style.font_size: 11.0
                                }
                                tab_close := Label{
                                    text: "×"
                                    draw_text.color: #x666666
                                    draw_text.text_style.font_size: 13.0
                                    width: 20 height: 20
                                    align: Align{x: 0.5 y: 0.5}
                                }
                            }

                            new_tab_btn := Button{
                                text: "+"
                                width: 28 height: 28
                                padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
                                margin: Inset{left: 2 right: 2 top: 2 bottom: 2}
                                draw_text.color: #x999999
                                draw_text.text_style.font_size: 16.0
                                draw_bg +: {
                                    pixel: fn() { return vec4(0.0, 0.0, 0.0, 0.0) }
                                }
                            }
                        }

                        // Window control buttons
                        window_controls := View{
                            width: Fit height: 32
                            flow: Right
                            align: Align{y: 0.0}

                            win_min := Button{
                                text: "—"
                                width: 46 height: 32
                                padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
                                draw_text.color: #x999999
                                draw_text.text_style.font_size: 10.0
                                align: Align{x: 0.5 y: 0.5}
                                draw_bg +: {
                                    pixel: fn() { return vec4(0.0, 0.0, 0.0, 0.0) }
                                }
                            }
                            win_max := Button{
                                text: "□"
                                width: 46 height: 32
                                padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
                                draw_text.color: #x999999
                                draw_text.text_style.font_size: 10.0
                                align: Align{x: 0.5 y: 0.5}
                                draw_bg +: {
                                    pixel: fn() { return vec4(0.0, 0.0, 0.0, 0.0) }
                                }
                            }
                            win_close := Button{
                                text: "X"
                                width: 46 height: 32
                                padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
                                draw_text.color: #x999999
                                draw_text.text_style.font_size: 11.0
                                align: Align{x: 0.5 y: 0.5}
                                draw_bg +: {
                                    pixel: fn() { return vec4(0.0, 0.0, 0.0, 0.0) }
                                }
                            }
                        }
                    }

                    // --- Navigation toolbar ---
                    toolbar := View{
                        flow: Right
                        event_order: Down
                        width: Fill height: Fit
                        padding: Inset{left: 8 right: 8 top: 4 bottom: 4}
                        spacing: 4
                        align: Align{y: 0.5}
                        draw_bg.color: #x2a2a2a
                        show_bg: true

                        back_btn := Button{ text: "←" }
                        forward_btn := Button{ text: "→" }
                        reload_btn := Button{ text: "↻" }

                        url_input := TextInput{
                            width: Fill height: Fit
                            empty_text: "Enter URC..."
                        }

                        go_btn := Button{ text: "Go" }
                        edit_btn := Button{ text: "✏" }
                        share_btn := Button{ text: "🔗" }
                        home_btn := Button{ text: "⌂" }

                        repo_mode_label := Label{
                            text: ""
                            draw_text.color: #x666666
                            draw_text.text_style.font_size: 9.0
                            width: Fit height: Fit
                            margin: Inset{left: 4 right: 0 top: 0 bottom: 0}
                        }
                    }

                    content_area := View{
                        width: Fill height: Fill
                        flow: Overlay

                        web_view := ServoWebView{
                            width: Fill
                            height: Fill
                        }

                        // Context menu overlay (starts off-screen; show_context_menu positions it)
                        context_menu := View{
                            visible: false
                            abs_pos: vec2(-1000.0, -1000.0)
                            width: Fit height: Fit
                            flow: Down
                            padding: Inset{left: 4 right: 4 top: 4 bottom: 4}
                            spacing: 2
                            show_bg: true
                            draw_bg.color: #x2a2a2a

                            context_copy_btn := Button{
                                text: "Copy"
                                width: 160 height: 28
                                padding: Inset{left: 12 right: 12 top: 4 bottom: 4}
                                draw_text.color: #xcccccc
                                draw_text.text_style.font_size: 12.0
                                draw_bg +: {
                                    color: uniform(#x2a2a2a)
                                    color_hover: uniform(#x3a3a3a)
                                    pixel: fn() {
                                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                        sdf.box(0.0 0.0 self.rect_size.x self.rect_size.y 4.0)
                                        sdf.fill(mix(self.color, self.color_hover, self.hover))
                                        return sdf.result
                                    }
                                }
                            }
                            context_edit_btn := Button{
                                text: "Go to Editor"
                                width: 160 height: 28
                                padding: Inset{left: 12 right: 12 top: 4 bottom: 4}
                                draw_text.color: #xcccccc
                                draw_text.text_style.font_size: 12.0
                                draw_bg +: {
                                    color: uniform(#x2a2a2a)
                                    color_hover: uniform(#x3a3a3a)
                                    pixel: fn() {
                                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                        sdf.box(0.0 0.0 self.rect_size.x self.rect_size.y 4.0)
                                        sdf.fill(mix(self.color, self.color_hover, self.hover))
                                        return sdf.result
                                    }
                                }
                            }
                        }
                    } // end content_area
                }
            }
        }
    }
    app
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum MakepadServoAction {
    None,
    Wake,
    /// A webview's page title changed.
    TitleChanged { webview_id: WebViewId, title: Option<String> },
    /// A webview's URL changed.
    UrlChanged { webview_id: WebViewId, url: String },
    /// A webview has new content to paint.
    NewFrameReady { webview_id: WebViewId },
    /// A webview was closed by page content (window.close()).
    WebViewClosed { webview_id: WebViewId },
}

impl Default for MakepadServoAction {
    fn default() -> Self {
        Self::None
    }
}

// ---------------------------------------------------------------------------
// WebViewDelegate — forwards Servo webview events to Makepad actions
// ---------------------------------------------------------------------------

pub(super) struct HaviWebViewDelegate;

impl servo::WebViewDelegate for HaviWebViewDelegate {
    fn notify_page_title_changed(&self, webview: servo::WebView, title: Option<String>) {
        Cx::post_action(MakepadServoAction::TitleChanged {
            webview_id: webview.id(),
            title,
        });
    }

    fn notify_url_changed(&self, webview: servo::WebView, url: servo::BrowserUrl) {
        Cx::post_action(MakepadServoAction::UrlChanged {
            webview_id: webview.id(),
            url: url.to_string(),
        });
    }

    fn notify_new_frame_ready(&self, webview: servo::WebView) {
        Cx::post_action(MakepadServoAction::NewFrameReady {
            webview_id: webview.id(),
        });
    }

    fn notify_closed(&self, webview: servo::WebView) {
        Cx::post_action(MakepadServoAction::WebViewClosed {
            webview_id: webview.id(),
        });
    }
}

// ---------------------------------------------------------------------------
// EventLoopWaker
// ---------------------------------------------------------------------------

struct MakepadEventLoopWaker;

impl servo::EventLoopWaker for MakepadEventLoopWaker {
    fn clone_box(&self) -> Box<dyn servo::EventLoopWaker> {
        Box::new(MakepadEventLoopWaker)
    }

    fn wake(&self) {
        Cx::post_action(MakepadServoAction::Wake);
    }
}

// ---------------------------------------------------------------------------
// ServoDelegate — auto-allow devtools connections
// ---------------------------------------------------------------------------

struct HaviServoDelegate;

impl servo::ServoDelegate for HaviServoDelegate {
    fn notify_devtools_server_started(&self, port: u16, _token: String) {
        eprintln!("HAVI_DEVTOOLS=127.0.0.1:{}", port);
        log!("DEVTOOLS_BIND=127.0.0.1:{} # havi-devtools-cli -p {}", port, port);
    }

    fn request_devtools_connection(&self, request: servo::AllowOrDenyRequest) {
        request.allow();
    }
}

// ---------------------------------------------------------------------------
// ResourceReader
// ---------------------------------------------------------------------------

struct ResourceReader;

impl servo::resources::ResourceReaderMethods for ResourceReader {
    #[cfg(not(target_os = "android"))]
    fn read(&self, file: servo::resources::Resource) -> Vec<u8> {
        let mut path = std::env::current_exe()
            .unwrap()
            .canonicalize()
            .unwrap();
        while path.pop() {
            path.push("resources");
            if path.is_dir() {
                path.push(file.filename());
                return std::fs::read(&path).expect("Can't read resource file");
            }
            path.pop();
        }
        panic!("Can't find resources directory");
    }

    #[cfg(target_os = "android")]
    fn read(&self, res: servo::resources::Resource) -> Vec<u8> {
        use servo::resources::Resource;
        Vec::from(match res {
            Resource::HstsPreloadList => {
                &include_bytes!("../resources/servo/hsts_preload.fstmap")[..]
            },
            Resource::BadCertHTML => &include_bytes!("../resources/servo/badcert.html")[..],
            Resource::NetErrorHTML => &include_bytes!("../resources/servo/neterror.html")[..],
            Resource::BrokenImageIcon => &include_bytes!("../resources/servo/rippy.png")[..],
            Resource::DomainList => {
                &include_bytes!("../resources/servo/public_domains.txt")[..]
            },
            Resource::BluetoothBlocklist => {
                &include_bytes!("../resources/servo/gatt_blocklist.txt")[..]
            },
            Resource::CrashHTML => &include_bytes!("../resources/servo/crash.html")[..],
            Resource::DirectoryListingHTML => {
                &include_bytes!("../resources/servo/directory-listing.html")[..]
            },
            Resource::AboutMemoryHTML => {
                &include_bytes!("../resources/servo/about-memory.html")[..]
            },
            Resource::DebuggerJS => &include_bytes!("../resources/servo/debugger.js")[..],
        })
    }

    fn sandbox_access_files_dirs(&self) -> Vec<std::path::PathBuf> {
        vec![]
    }

    fn sandbox_access_files(&self) -> Vec<std::path::PathBuf> {
        vec![]
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

app_main!(App);

impl App {
    fn run(vm: &mut ScriptVm) -> Self {
        crate::makepad_widgets::script_mod(vm);
        crate::servo_web_view::script_mod(vm);
        App::from_script_mod(vm, self::script_mod)
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,

    #[rust]
    _embedded_hpprd: Option<EmbeddedHpprd>,
    #[rust]
    servo: Option<servo::Servo>,
    #[rust]
    rendering_context: Option<Rc<servo::MakepadRenderingContext>>,
    #[rust]
    texture: Option<Texture>,
    #[rust]
    next_frame: NextFrame,
    #[rust]
    content_size: (usize, usize),
    #[rust]
    initialized: bool,
    #[rust]
    dpi_factor: f64,

    // --- Performance optimization state ---
    /// Whether Servo has signaled that new content is available and needs painting.
    #[rust]
    needs_paint: bool,
    /// Number of frames since last activity. Used for idle detection to stop the
    /// frame loop when nothing is happening.
    #[rust]
    idle_frames: u32,

    // --- Touch gesture recognition state (used in handle_actions) ---
    /// Position (logical pixels) of the finger when it went down. Used to
    /// distinguish taps from scroll gestures.
    #[rust]
    finger_down_pos: Option<DVec2>,
    /// Whether the current finger gesture has been recognized as a scroll
    /// (finger moved beyond the tap threshold). Once true, Touch events are
    /// sent for the remainder of the gesture.
    #[rust]
    is_touch_scrolling: bool,
    /// Whether the current gesture originated from a mouse device. Mouse
    /// drags send MouseDown + MouseMove + MouseUp (for text selection)
    /// instead of Touch events (for scrolling).
    #[rust]
    is_mouse_gesture: bool,
    /// Whether a mouse drag is in progress (MouseDown sent to Servo).
    #[rust]
    is_mouse_dragging: bool,
    /// Whether the current gesture is a right-click. Right-clicks are sent
    /// to Servo immediately on finger-down; finger-up is suppressed.
    #[rust]
    is_right_click_gesture: bool,

    // --- Tab state ---
    #[rust]
    scroll_y_estimate: f64,
    #[rust]
    content_height_estimate: f64,

    // --- Context menu state ---
    #[rust]
    context_menu_open: bool,
    #[rust]
    context_menu_pos: DVec2,
    // --- Tab state ---
    #[rust]
    tabs: Vec<TabInfo>,
    #[rust]
    active_tab_idx: usize,

    // --- IPC single-instance listener ---
    #[rust]
    ipc_rx: Option<std::sync::mpsc::Receiver<havi_protocols::instance::IpcCommand>>,
}

/// Maximum number of idle frames before stopping the frame loop.
/// When the frame loop stops, Servo's `wake()` call will restart it.
const MAX_IDLE_FRAMES: u32 = 10;

/// Distance threshold (in logical pixels) to distinguish taps from scrolls.
/// If the finger moves more than this distance from the initial touch point,
/// the gesture is treated as a scroll; otherwise it's a tap (click).
const TAP_DISTANCE_THRESHOLD: f64 = 5.0;

/// Create a rendering context that loads GL function pointers from Makepad's EGL
/// context. Servo renders into an FBO within the same GL context — no second
/// context is created.
///
/// On Linux, Makepad exposes EGL via `cx.os.opengl_cx` (OpenglCx). On Android,
/// it uses `cx.os.display` (CxAndroidDisplay).
#[cfg(target_os = "linux")]
fn create_shared_rendering_context(
    cx: &mut Cx,
    size: dpi::PhysicalSize<u32>,
) -> Result<servo::MakepadRenderingContext, servo::rendering_context::Error> {
    let opengl_cx = cx
        .os
        .opengl_cx
        .as_ref()
        .expect("Makepad OpenGL context not initialized");

    opengl_cx.make_current();

    let egl_get_proc_address = opengl_cx
        .libegl
        .eglGetProcAddress
        .expect("eglGetProcAddress not available");

    // SAFETY: Makepad's EGL context is current (ensured above). The GL function
    // pointers loaded via eglGetProcAddress are valid for this context.
    unsafe {
        servo::MakepadRenderingContext::new_from_loader(size, &|func_name: &str| {
            let c_name = std::ffi::CString::new(func_name).unwrap();
            egl_get_proc_address(c_name.as_ptr()) as *const std::ffi::c_void
        })
    }
}

#[cfg(target_os = "android")]
fn create_shared_rendering_context(
    cx: &mut Cx,
    size: dpi::PhysicalSize<u32>,
) -> Result<servo::MakepadRenderingContext, servo::rendering_context::Error> {
    let display = cx
        .os
        .display
        .as_ref()
        .expect("Makepad Android display not initialized");

    display.make_current();

    let egl_get_proc_address = display
        .libegl
        .eglGetProcAddress
        .expect("eglGetProcAddress not available");

    // SAFETY: Makepad's EGL context is current (ensured above). The GL function
    // pointers loaded via eglGetProcAddress are valid for this context.
    unsafe {
        servo::MakepadRenderingContext::new_from_loader(size, &|func_name: &str| {
            let c_name = std::ffi::CString::new(func_name).unwrap();
            egl_get_proc_address(c_name.as_ptr()) as *const std::ffi::c_void
        })
    }
}

/// Restore Makepad's EGL context as current after Servo rendering.
/// With the unified single-context approach this is technically a no-op (Servo
/// renders within Makepad's own context), but kept for safety in case platform
/// code changes the current context between frames.
#[cfg(target_os = "linux")]
fn restore_makepad_gl_context(cx: &mut Cx) {
    if let Some(opengl_cx) = cx.os.opengl_cx.as_ref() {
        opengl_cx.make_current();
    }
}

#[cfg(target_os = "android")]
fn restore_makepad_gl_context(cx: &mut Cx) {
    if let Some(display) = cx.os.display.as_ref() {
        display.make_current();
    }
}

impl App {
    fn init_servo(&mut self, cx: &mut Cx) {
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

        self.initialized = true;
        self.dpi_factor = dpi_factor;

        // Init crypto provider
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .ok();

        // Init resource reader
        servo::resources::set(Box::new(ResourceReader));

        // Use physical pixel dimensions for the initial texture.
        // Makepad's inner_size is in logical pixels; multiply by DPI for physical.
        let width = ((inner.x * self.dpi_factor) as u32).max(64);
        let height = ((inner.y * self.dpi_factor) as u32).max(64);
        self.content_size = (width as usize, height as usize);

        // Create a rendering context with a GL context that shares textures with Makepad's.
        // This enables zero-copy texture sharing: Servo renders to an FBO texture that
        // Makepad can directly bind and draw without any GPU→CPU→GPU roundtrip.
        // Step 1: Create Makepad-owned render texture FIRST (physical pixel dimensions).
        let (texture, gl_texture_id) = cx.create_gl_render_texture(width as usize, height as usize);

        // Step 2: Extract EGL handles and create the shared rendering context.
        // Platform-specific: Linux uses OpenglCx with Wayland/X11, Android uses CxAndroidDisplay.
        let size = dpi::PhysicalSize::new(width, height);
        let rendering_context = match create_shared_rendering_context(cx, size) {
            Ok(rc) => Rc::new(rc),
            Err(e) => {
                log!("[havishell] FAILED to create rendering context: {:?}", e);
                return;
            }
        };

        // Switch the rendering context to use Makepad's texture as its render target
        rendering_context.set_external_texture(gl_texture_id, size);

        // Restore Makepad's GL context after creating the shared context.
        // create_shared_rendering_context + set_external_texture leave Servo's
        // context current; Makepad needs its own context for subsequent draw passes.
        restore_makepad_gl_context(cx);

        // Determine repo target: HAVI_REPO env var (external) or embedded hpprd.
        let embedded_hpprd = match std::env::var("HAVI_REPO").ok().filter(|v| !v.trim().is_empty()) {
            Some(value) => {
                // External mode: parse "tcp+host:port" style spec.
                let target = hppr_client::env_target::parse_via(&value)
                    .expect("invalid HAVI_REPO value");
                hppr_client::set_repo_target(target);
                log!("[havishell] External hpprd via HAVI_REPO={}", value);
                None
            },
            None => {
                // Embedded mode: start local hpprd.
                let repo_path = havi_protocols::config::repo_dir();
                match EmbeddedHpprd::start(repo_path) {
                    Ok(HpprdMode::Embedded(h)) => {
                        let port = h.port();
                        let target = hppr_client::ViaSpec::Net {
                            host: "127.0.0.1".to_string(),
                            port,
                            scheme: Some(hppr_client::env_target::TransportScheme::Tcp),
                        };
                        hppr_client::set_repo_target(target);
                        log!("[havishell] Embedded hpprd on localhost:{}", port);
                        Some(h)
                    },
                    Ok(HpprdMode::Reused { socket_path }) => {
                        let target = hppr_client::ViaSpec::Unix {
                            path: socket_path.clone().into(),
                        };
                        hppr_client::set_repo_target(target);
                        log!("[havishell] Reusing existing hpprd via {}", socket_path);
                        None
                    },
                    Err(e) => {
                        log!("[havishell] Failed to start embedded hpprd: {}", e);
                        None
                    },
                }
            },
        };

        // Initialize HPPR protocol handlers
        let hppr_handler = {
            let target = hppr_client::repo_target().clone();
            Arc::new(
                havi_protocols::client::HpprdClientAsync::new(target)
                    .expect("invalid repo endpoint"),
            )
        };
        let credential_store = global_credential_store();

        // Bootstrap credentials
        {
            let endpoint = hppr_client::repo_endpoint().to_string();
            match hppr_client::connect_tcp(&endpoint, hppr_client::Signer::anyone()) {
                Ok(mut client) => {
                    if let Ok(greeting) = client.hello() {
                        let key = greeting.verifying_key();
                        if credential_store.load_admin_for_key(key).is_err() {
                            credential_store.bootstrap_admin();
                            let _ = credential_store.persist_admin_for_key(key);
                        }
                    } else {
                        credential_store.bootstrap_admin();
                    }
                },
                Err(_) => credential_store.bootstrap_admin(),
            }
        }

        let mut protocol_registry = ProtocolRegistry::default();
        let _ = protocol_registry.register(
            "hppr",
            crate::protocols::hppr::HpprHandler::new(hppr_handler.clone(), credential_store.clone()),
        );
        let _ = protocol_registry.register(
            "havi",
            crate::protocols::havi::HaviHandler::new(hppr_handler.clone(), credential_store.clone()),
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

        self._embedded_hpprd = embedded_hpprd;

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
            .build();
        servo.set_delegate(Rc::new(HaviServoDelegate));
        servo.setup_logging();

        // Step 4: Create first WebView with proper HiDPI scale factor.
        let start_url_str = std::env::var("HAVI_URL")
            .unwrap_or_else(|_| HOME_URL.to_string());
        let url = servo::BrowserUrl::parse(&start_url_str).unwrap();
        let hidpi: Scale<f32, DeviceIndependentPixel, DevicePixel> =
            Scale::new(self.dpi_factor as f32);
        let webview = servo::WebViewBuilder::new(&servo, rendering_context.clone())
            .url(url)
            .hidpi_scale_factor(hidpi)
            .delegate(Rc::new(HaviWebViewDelegate))
            .build();

        let webview_id = webview.id();
        self.tabs.push(TabInfo {
            webview_id,
            webview,
            title: title_from_url(&start_url_str),
            url: start_url_str.clone(),
            widget_id: next_tab_live_id(),
        });
        self.active_tab_idx = 0;

        self.servo = Some(servo);
        self.rendering_context = Some(rendering_context);

        // Step 5: Assign texture to the ServoWebView widget
        self.texture = Some(texture);
        if let Some(texture) = &self.texture {
            self.ui
                .servo_web_view(cx, ids!(web_view))
                .set_texture(cx, Some(texture.clone()));
        }

        // Set initial URL in the text input
        self.ui.text_input(cx, ids!(url_input)).set_text(cx, &start_url_str);

        // Show repo mode indicator
        let mode_text = if self._embedded_hpprd.is_some() { "embedded" } else { "connected" };
        self.ui.label(cx, ids!(repo_mode_label)).set_text(cx, mode_text);

        // Set window title caption to "havi"
        self.ui.widget(cx, ids!(caption_bar.caption_label.label)).set_text(cx, "havi");

        // Sync tab bar UI
        self.sync_tab_bar(cx);

        // Hide the Window's built-in caption bar — we use our own tab_bar_wrap
        self.ui.view(cx, ids!(caption_bar)).set_visible(cx, false);

        // In Makepad Studio's RunView, window control buttons are meaningless —
        // the child process doesn't own a real window.
        if cx.in_makepad_studio {
            self.ui.view(cx, ids!(window_controls)).set_visible(cx, false);
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

        // Print eval-compatible environment summary
        {
            let repo_dir = havi_protocols::config::repo_dir();
            eprintln!("HPPRD_REPO={}", repo_dir.display());
            if let Some(ref h) = self._embedded_hpprd {
                let port = h.port();
                eprintln!("HPPRD_BIND=127.0.0.1:{}", port);
                eprintln!("HPPRD_BIND_WS=127.0.0.1:{}", port + 1);
                eprintln!("HPPRD_BIND_QUIB=127.0.0.1:{}", port.saturating_sub(1));
                eprintln!("HPPRD_BIND_UDP=127.0.0.1:{}", port);
                eprintln!("HPPRD_SOCK={}", repo_dir.join("hppr.sock").display());
            }
            eprintln!("HAVI_URL={}", start_url_str);
        }

        // Start the frame loop
        self.next_frame = cx.new_next_frame();

        // Control mode: stdin/stdout JSON protocol.
        // Skip when running inside Makepad Studio's RunView — stdin is already
        // used by the Studio WebSocket protocol.
        if std::env::var("HAVI_CONTROL").is_ok() && !cx.in_makepad_studio {
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
                    if line.is_empty() { continue; }
                    match StudioToApp::deserialize_json(&line) {
                        Ok(msg) => {
                            if tx.send(msg).is_err() { break; }
                            SignalToUI::set_ui_signal();
                        }
                        Err(e) => {
                            eprintln!("[havi-control] parse error: {:?} for: {}", e, line);
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

        // IMPORTANT: Notify the webview BEFORE updating the rendering context's
        // external texture. webview.resize() → resize_rendering_context() checks
        // if rendering_context.size() == new_size to decide whether to update
        // WebRender's document view. If we call set_external_texture first, it
        // updates the stored size, making the check see matching sizes and skip
        // set_document_view — so WebRender never learns the new viewport.
        // Resize all webviews so they're ready when switched to.
        for tab in &self.tabs {
            tab.webview.resize(phys_size);
        }

        // Create a new Makepad-owned texture at the new size.
        // The old texture is dropped and Makepad cleans up its GL resources.
        let (texture, gl_texture_id) =
            cx.create_gl_render_texture(new_width as usize, new_height as usize);

        // Tell Servo's rendering context to render into the new texture
        if let Some(rc) = &self.rendering_context {
            rc.set_external_texture(gl_texture_id, phys_size);
            // Restore Makepad's context after set_external_texture (which uses Servo's context)
            restore_makepad_gl_context(cx);
        }

        // Assign new texture to ServoWebView widget
        self.texture = Some(texture);
        if let Some(texture) = &self.texture {
            self.ui
                .servo_web_view(cx, ids!(web_view))
                .set_texture(cx, Some(texture.clone()));
        }

        // Force a repaint at the new size
        self.needs_paint = true;
    }

    /// Main update method called each frame. Spins Servo's event loop and
    /// optionally does the expensive paint + readback cycle.
    fn update_servo_and_texture(&mut self, cx: &mut Cx) {
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
                let new_title = tab.webview.page_title()
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

        // Only do the expensive paint + readback cycle when Servo has new content
        if !self.needs_paint {
            self.idle_frames = self.idle_frames.saturating_add(1);
            return;
        }
        self.needs_paint = false;
        self.idle_frames = 0;

        let active_webview = self.tabs.get(self.active_tab_idx).map(|t| &t.webview);
        if let (Some(webview), Some(rc)) = (active_webview, &self.rendering_context) {
            // Tell WebRender to render the current state.
            // This renders to the shared GL context's FBO texture.
            webview.paint();

            // Flush Servo's GL command queue so the texture contents are visible
            // when Makepad's GL context samples it.
            rc.present();

            // Restore Makepad's EGL context. Servo's paint() + present() left
            // Servo's shared GL context current; Makepad's draw passes need their
            // own context to be current so they can see the texture we rendered into.
            restore_makepad_gl_context(cx);
        }
    }

    fn point_to_device(&self, cx: &mut Cx, pos: DVec2) -> servo::DevicePoint {
        let rect = self.ui.servo_web_view(cx, ids!(web_view)).area().rect(cx);
        // pos and rect are in Makepad logical pixels; Servo wants device pixels.
        let x = ((pos.x - rect.pos.x) * self.dpi_factor) as f32;
        let y = ((pos.y - rect.pos.y) * self.dpi_factor) as f32;
        servo::DevicePoint::new(x, y)
    }

    /// Get the active tab's webview, if any.
    fn active_webview(&self) -> Option<&servo::WebView> {
        self.tabs.get(self.active_tab_idx).map(|t| &t.webview)
    }

    fn send_input_event(&self, event: servo::InputEvent) {
        if let Some(webview) = self.active_webview() {
            webview.notify_input_event(event);
            if let Some(servo) = &self.servo {
                servo.spin_event_loop();
            }
        }
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
        if self.ui.button(cx, ids!(share_btn)).clicked(actions) {
            // Copy current URL to clipboard
            let url_text = self.ui.text_input(cx, ids!(url_input)).text();
            cx.copy_to_clipboard(&url_text);
        }
        if self.ui.button(cx, ids!(home_btn)).clicked(actions) {
            nav_action = Some(NavCommand::Navigate(HOME_URL.into()));
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

        // --- Tab bar events ---
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
                }
                Some(MakepadServoAction::TitleChanged { webview_id, title }) => {
                    let webview_id = *webview_id;
                    let title = title.clone();
                    if let Some(idx) = self.tab_index_for_webview(webview_id) {
                        self.tabs[idx].title = title.clone()
                            .unwrap_or_else(|| title_from_url(&self.tabs[idx].url));
                        self.sync_tab_bar(cx);
                    }
                }
                Some(MakepadServoAction::UrlChanged { webview_id, url }) => {
                    let webview_id = *webview_id;
                    let url = url.clone();
                    if let Some(idx) = self.tab_index_for_webview(webview_id) {
                        self.tabs[idx].url = url.clone();
                        if idx == self.active_tab_idx {
                            self.ui.text_input(cx, ids!(url_input)).set_text(cx, &url);
                        }
                    }
                }
                Some(MakepadServoAction::NewFrameReady { webview_id }) => {
                    let webview_id = *webview_id;
                    // Only repaint if the active webview has new content
                    if self.tabs.get(self.active_tab_idx)
                        .map_or(false, |t| t.webview_id == webview_id)
                    {
                        self.needs_paint = true;
                        self.idle_frames = 0;
                        self.next_frame = cx.new_next_frame();
                        cx.redraw_all();
                    }
                }
                Some(MakepadServoAction::WebViewClosed { webview_id }) => {
                    let webview_id = *webview_id;
                    if let Some(idx) = self.tab_index_for_webview(webview_id) {
                        self.close_tab(cx, idx);
                    }
                }
                _ => {}
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

        // Handle next-frame for servo update loop
        if let Some(_ne) = self.next_frame.is_event(event) {
            self.update_servo_and_texture(cx);

            // Tick scroll fade animation
            let scroll_fading = self.ui.servo_web_view(cx, ids!(web_view))
                .tick_scroll_fade(cx, 1.0 / 60.0);

            // Continue the frame loop while there's recent activity.
            // When idle, stop to save CPU/GPU. The Wake action will restart it.
            // Keep running in control mode so stdin messages are polled.
            if self.idle_frames < MAX_IDLE_FRAMES || scroll_fading
                || Cx::has_studio_web_socket()
            {
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
                        for (child_id, child_widget) in tab_bar.children.iter() {
                            if *child_id == live_id!(tab_template) { continue; }
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



