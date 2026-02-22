use euclid::Scale;
use makepad_widgets::*;
use servo::{
    CompositionEvent, CompositionState, DeviceIndependentPixel, DevicePixel,
    ImeEvent, Key, KeyState, KeyboardEvent,
    MouseButton, MouseButtonAction, MouseButtonEvent, MouseLeftViewportEvent,
    NamedKey, RenderingContext, TouchEventType, TouchId,
    WebViewId, percent_decode_jsonqa,
};
use servo::protocol_handler::ProtocolRegistry;
use havi_protocols::embedded_hpprd::EmbeddedHpprd;
use havi_protocols::credentials::global_credential_store;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use makepad_widgets::makepad_platform::studio::StudioToApp;
use makepad_widgets::makepad_platform::thread::SignalToUI;
use makepad_widgets::makepad_platform::makepad_micro_serde::DeJson;

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
                    }

                    web_view := ServoWebView{
                        width: Fill
                        height: Fill
                    }

                    // Context menu overlay (hidden by default)
                    context_menu := View{
                        visible: false
                        abs_pos: vec2(0.0, 0.0)
                        width: Fit height: Fit
                        flow: Down
                        padding: Inset{left: 4 right: 4 top: 4 bottom: 4}
                        spacing: 2
                        show_bg: true
                        draw_bg.color: #x2a2a2a

                        context_copy_btn := Button{
                            text: "Copy"
                            width: 120 height: 28
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

struct HaviWebViewDelegate;

impl servo::WebViewDelegate for HaviWebViewDelegate {
    fn notify_page_title_changed(&self, webview: servo::WebView, title: Option<String>) {
        Cx::post_action(MakepadServoAction::TitleChanged {
            webview_id: webview.id(),
            title,
        });
    }

    fn notify_url_changed(&self, webview: servo::WebView, url: url::Url) {
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
        log!("DEVTOOLS_BIND=127.0.0.1:{} # havi-webview-remote-cli -p {}", port, port);
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
// Tab state
// ---------------------------------------------------------------------------

/// Default start page URL.
const HOME_URL: &str = "hppr://u/web/index.html";

/// Derive a tab title from a URL. Uses the last path segment.
fn title_from_url(url: &str) -> String {
    url.rsplit('/').find(|s| !s.is_empty())
        .unwrap_or("New Tab")
        .to_string()
}

/// Counter for generating unique tab widget LiveIds.
static TAB_ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_tab_live_id() -> LiveId {
    LiveId(TAB_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

struct TabInfo {
    webview_id: WebViewId,
    webview: servo::WebView,
    title: String,
    url: String,
    /// LiveId used as the key in tab_bar View.children.
    widget_id: LiveId,
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

}

/// Maximum number of idle frames before stopping the frame loop.
/// When the frame loop stops, Servo's `wake()` call will restart it.
const MAX_IDLE_FRAMES: u32 = 10;

/// Distance threshold (in logical pixels) to distinguish taps from scrolls.
/// If the finger moves more than this distance from the initial touch point,
/// the gesture is treated as a scroll; otherwise it's a tap (click).
const TAP_DISTANCE_THRESHOLD: f64 = 5.0;

/// Create a shared GL rendering context by extracting EGL handles from Makepad's
/// platform-specific context. This context shares texture namespaces with Makepad,
/// enabling zero-copy rendering: Servo renders into an FBO texture that Makepad
/// can directly bind and display.
///
/// On Linux, Makepad exposes EGL via `cx.os.opengl_cx` (OpenglCx) with Wayland/X11
/// platform info. On Android, it uses `cx.os.display` (CxAndroidDisplay) with a
/// direct EGL backend.
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
    let egl_display = opengl_cx.egl_display as *mut std::ffi::c_void;
    let egl_context = opengl_cx.egl_context as *mut std::ffi::c_void;
    let egl_platform = opengl_cx.egl_platform;
    let platform_display = opengl_cx.egl_platform_display;

    // Ensure Makepad's EGL context is current before creating the shared
    // rendering context. Surfman's create_context_from_native_context
    // internally queries GL_VERSION via glow, which requires a current context.
    opengl_cx.make_current();

    // SAFETY: The EGL handles are valid and the context is current (ensured above).
    unsafe {
        servo::MakepadRenderingContext::new(
            egl_display,
            egl_context,
            egl_platform,
            platform_display,
            size,
        )
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

    // Ensure Makepad's EGL context is current.
    display.make_current();

    // Get eglGetProcAddress from Makepad's loaded EGL library.
    // We pass this as a GL function loader so Servo can load gleam/glow
    // function pointers without creating a second EGL context.
    let egl_get_proc_address = display
        .libegl
        .eglGetProcAddress
        .expect("eglGetProcAddress not available");

    // SAFETY: Makepad's EGL context is current (ensured above) and valid.
    // The GL function pointers loaded via eglGetProcAddress are valid for
    // this context. No second EGL context is created.
    unsafe {
        servo::MakepadRenderingContext::new_android(size, &|func_name: &str| {
            let c_name = std::ffi::CString::new(func_name).unwrap();
            egl_get_proc_address(c_name.as_ptr()) as *const std::ffi::c_void
        })
    }
}

/// Restore Makepad's EGL context as current after Servo rendering.
/// Servo creates its own shared GL context for painting. After Servo's paint/present
/// cycle, that context is left current. Makepad needs its own context current to draw.
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
                    Ok(h) => {
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
        let url = url::Url::parse(&start_url_str).unwrap();
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

        // Set window title caption to "havi"
        self.ui.widget(cx, ids!(caption_bar.caption_label.label)).set_text(cx, "havi");

        // Sync tab bar UI
        self.sync_tab_bar(cx);

        // Hide the Window's built-in caption bar — we use our own tab_bar_wrap
        self.ui.view(cx, ids!(caption_bar)).set_visible(cx, false);

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

        // Control mode: stdin/stdout JSON protocol
        if std::env::var("HAVI_CONTROL").is_ok() {
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
                    let new_url_str = percent_decode_jsonqa(new_url.as_str());
                    if new_url_str != tab.url {
                        tab.url = new_url_str;
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

    fn navigate(&self, url_str: &str) {
        if let Some(webview) = self.active_webview() {
            if let Ok(url) = url::Url::parse(url_str) {
                webview.load(url);
            } else if let Ok(url) = url::Url::parse(&format!("https://{}", url_str)) {
                webview.load(url);
            }
        }
    }

    fn go_back(&self) {
        if let Some(webview) = self.active_webview() {
            webview.go_back(1);
        }
    }

    fn go_forward(&self) {
        if let Some(webview) = self.active_webview() {
            webview.go_forward(1);
        }
    }

    fn reload(&self) {
        if let Some(webview) = self.active_webview() {
            webview.reload();
        }
    }

    /// Synchronize the tab bar UI: rebuild children from tab state.
    fn sync_tab_bar(&mut self, cx: &mut Cx) {
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
    fn handle_tab_clicks(&mut self, cx: &mut Cx, actions: &Actions) {
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
    fn create_webview(&self, url_str: &str) -> Option<servo::WebView> {
        let servo = self.servo.as_ref()?;
        let rc = self.rendering_context.as_ref()?;
        let url = url::Url::parse(url_str).ok()?;
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
    fn activate_tab_webview(&self, active_idx: usize) {
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
    fn add_tab(&mut self, cx: &mut Cx) {
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
        });
        self.active_tab_idx = self.tabs.len() - 1;
        self.activate_tab_webview(self.active_tab_idx);
        self.ui.text_input(cx, ids!(url_input)).set_text(cx, HOME_URL);
        self.needs_paint = true;
        self.sync_tab_bar(cx);
    }

    /// Close tab at given index. Quits when the last tab is closed.
    fn close_tab(&mut self, cx: &mut Cx, idx: usize) {
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
    fn switch_tab(&mut self, cx: &mut Cx, idx: usize) {
        if idx >= self.tabs.len() || idx == self.active_tab_idx {
            return;
        }
        self.active_tab_idx = idx;
        self.activate_tab_webview(idx);
        let url = self.tabs[idx].url.clone();
        self.ui.text_input(cx, ids!(url_input)).set_text(cx, &url);
        self.needs_paint = true;
        self.sync_tab_bar(cx);
    }

    /// Find tab index by webview id.
    fn tab_index_for_webview(&self, webview_id: WebViewId) -> Option<usize> {
        self.tabs.iter().position(|t| t.webview_id == webview_id)
    }

    /// Show the context menu at the right-click position.
    fn show_context_menu(&mut self, cx: &mut Cx) {
        let menu = self.ui.view(cx, ids!(context_menu));
        menu.set_visible(cx, true);
        if let Some(mut v) = menu.borrow_mut() {
            v.walk.abs_pos = Some(dvec2(self.context_menu_pos.x, self.context_menu_pos.y));
        }
        cx.redraw_all();
    }

    fn hide_context_menu(&mut self, cx: &mut Cx) {
        self.context_menu_open = false;
        self.ui.view(cx, ids!(context_menu)).set_visible(cx, false);
        cx.redraw_all();
    }

    /// Send Ctrl+C to Servo to copy selected text.
    fn send_copy_command(&self) {
        use keyboard_types::{Code, Modifiers};
        // Send Ctrl+C keydown
        self.send_input_event(servo::InputEvent::Keyboard(
            KeyboardEvent::new(keyboard_types::KeyboardEvent {
                state: keyboard_types::KeyState::Down,
                key: Key::Character("c".into()),
                code: Code::KeyC,
                location: keyboard_types::Location::Standard,
                modifiers: Modifiers::CONTROL,
                repeat: false,
                is_composing: false,
            }),
        ));
        // Send Ctrl+C keyup
        self.send_input_event(servo::InputEvent::Keyboard(
            KeyboardEvent::new(keyboard_types::KeyboardEvent {
                state: keyboard_types::KeyState::Up,
                key: Key::Character("c".into()),
                code: Code::KeyC,
                location: keyboard_types::Location::Standard,
                modifiers: Modifiers::CONTROL,
                repeat: false,
                is_composing: false,
            }),
        ));
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
            // Navigate to the editor view for the current page
            let url_text = self.ui.text_input(cx, ids!(url_input)).text();
            if let Ok(url) = url::Url::parse(&url_text) {
                let edit_url = format!("hppr-editor://{}{}", url.host_str().unwrap_or(""), url.path());
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

        // Handle ServoWebView actions — the custom widget emits these for all
        // touch / mouse / keyboard / IME interactions on the web content area.
        let mut handled_input = false;
        for action in actions {
            if let Some(wa) = action.as_widget_action() {
                let swva: ServoWebViewAction = wa.cast();
                match &swva {
                    ServoWebViewAction::None => {}

                    // ----- Finger / touch -----
                    //
                    // Strategy: defer the Touch(Down) until we know whether
                    // the gesture is a tap or a scroll.
                    //
                    //  • TAP  → send only Mouse events (move + down + up).
                    //           This avoids the double-fire problem where
                    //           both touchend JS listeners AND the synthetic
                    //           click fire on toggle-style UI (hamburger menus).
                    //
                    //  • SCROLL → send Touch(Down) retroactively at the saved
                    //             position, then Touch(Move) for each move,
                    //             and Touch(Up) at the end.
                    //
                    ServoWebViewAction::FingerDown { abs, digit_id: _, is_mouse, is_right_click } => {
                        if *is_right_click {
                            self.context_menu_pos = *abs;
                            self.context_menu_open = true;
                            self.show_context_menu(cx);
                            handled_input = true;
                        } else {
                            // Close context menu on left click
                            if self.context_menu_open {
                                self.hide_context_menu(cx);
                            }
                            self.finger_down_pos = Some(*abs);
                            self.is_touch_scrolling = false;
                            self.is_mouse_gesture = *is_mouse;
                            self.is_mouse_dragging = false;
                            // Don't send any event yet — wait to see if it's a tap or drag/scroll.
                            handled_input = true;
                        }
                    }
                    ServoWebViewAction::FingerUp { abs, digit_id, is_mouse: _ } => {
                        let pt = self.point_to_device(cx, *abs);
                        let touch_id = TouchId(*digit_id as i32);
                        if self.is_mouse_dragging {
                            // Complete mouse drag — send final MouseMove + MouseUp
                            self.send_input_event(servo::InputEvent::MouseMove(
                                servo::MouseMoveEvent::new(pt.into()),
                            ));
                            self.send_input_event(servo::InputEvent::MouseButton(
                                MouseButtonEvent::new(
                                    MouseButtonAction::Up,
                                    MouseButton::Left,
                                    pt.into(),
                                ),
                            ));
                        } else if self.is_touch_scrolling {
                            // Complete the touch/scroll sequence
                            self.send_input_event(servo::InputEvent::Touch(
                                servo::TouchEvent::new(
                                    TouchEventType::Up,
                                    touch_id,
                                    pt.into(),
                                ),
                            ));
                        } else {
                            // TAP — send mouse click only (no touch events)
                            self.send_input_event(servo::InputEvent::MouseMove(
                                servo::MouseMoveEvent::new(pt.into()),
                            ));
                            self.send_input_event(servo::InputEvent::MouseButton(
                                MouseButtonEvent::new(
                                    MouseButtonAction::Down,
                                    MouseButton::Left,
                                    pt.into(),
                                ),
                            ));
                            self.send_input_event(servo::InputEvent::MouseButton(
                                MouseButtonEvent::new(
                                    MouseButtonAction::Up,
                                    MouseButton::Left,
                                    pt.into(),
                                ),
                            ));
                        }

                        // Reset gesture state
                        self.finger_down_pos = None;
                        self.is_touch_scrolling = false;
                        self.is_mouse_gesture = false;
                        self.is_mouse_dragging = false;
                        handled_input = true;
                    }
                    ServoWebViewAction::FingerMove { abs, digit_id, is_mouse: _ } => {
                        let touch_id = TouchId(*digit_id as i32);

                        if !self.is_touch_scrolling && !self.is_mouse_dragging {
                            if let Some(down_pos) = self.finger_down_pos {
                                let dx = abs.x - down_pos.x;
                                let dy = abs.y - down_pos.y;
                                let dist = (dx * dx + dy * dy).sqrt();
                                if dist > TAP_DISTANCE_THRESHOLD {
                                    if self.is_mouse_gesture {
                                        // Mouse drag — send MouseDown at original position
                                        self.is_mouse_dragging = true;
                                        let down_pt = self.point_to_device(cx, down_pos);
                                        self.send_input_event(servo::InputEvent::MouseButton(
                                            MouseButtonEvent::new(
                                                MouseButtonAction::Down,
                                                MouseButton::Left,
                                                down_pt.into(),
                                            ),
                                        ));
                                    } else {
                                        // Touch scroll
                                        self.is_touch_scrolling = true;
                                        let down_pt = self.point_to_device(cx, down_pos);
                                        self.send_input_event(servo::InputEvent::Touch(
                                            servo::TouchEvent::new(
                                                TouchEventType::Down,
                                                touch_id,
                                                down_pt.into(),
                                            ),
                                        ));
                                    }
                                }
                            }
                        }

                        if self.is_mouse_dragging {
                            let pt = self.point_to_device(cx, *abs);
                            self.send_input_event(servo::InputEvent::MouseMove(
                                servo::MouseMoveEvent::new(pt.into()),
                            ));
                        } else if self.is_touch_scrolling {
                            let pt = self.point_to_device(cx, *abs);
                            self.send_input_event(servo::InputEvent::Touch(
                                servo::TouchEvent::new(
                                    TouchEventType::Move,
                                    touch_id,
                                    pt.into(),
                                ),
                            ));
                        }
                        handled_input = true;
                    }

                    // ----- Mouse hover events -----
                    ServoWebViewAction::HoverIn { abs }
                    | ServoWebViewAction::HoverOver { abs } => {
                        let pt = self.point_to_device(cx, *abs);
                        self.send_input_event(servo::InputEvent::MouseMove(
                            servo::MouseMoveEvent::new(pt.into()),
                        ));
                        handled_input = true;
                    }
                    ServoWebViewAction::HoverOut => {
                        self.send_input_event(servo::InputEvent::MouseLeftViewport(
                            MouseLeftViewportEvent::default(),
                        ));
                        handled_input = true;
                    }

                    // ----- Scroll / wheel events -----
                    ServoWebViewAction::Scroll { abs, scroll } => {
                        let pt = self.point_to_device(cx, *abs);
                        let delta = servo::WheelDelta {
                            x: scroll.x * self.dpi_factor,
                            y: scroll.y * self.dpi_factor,
                            z: 0.0,
                            mode: servo::WheelMode::DeltaPixel,
                        };
                        self.send_input_event(servo::InputEvent::Wheel(
                            servo::WheelEvent::new(delta, pt.into()),
                        ));
                        // Update local scroll estimate for the overlay indicator.
                        // scroll.y is in logical pixels (negative = scroll down in Makepad).
                        self.scroll_y_estimate = (self.scroll_y_estimate - scroll.y).max(0.0);
                        // Use viewport size as rough content height estimate until we know better.
                        let vp_h = self.ui.servo_web_view(cx, ids!(web_view)).area().rect(cx).size.y;
                        if self.content_height_estimate < vp_h {
                            self.content_height_estimate = vp_h * 3.0; // rough initial guess
                        }
                        // Clamp scroll to content bounds
                        let max_scroll = (self.content_height_estimate - vp_h).max(0.0);
                        self.scroll_y_estimate = self.scroll_y_estimate.min(max_scroll);
                        self.ui.servo_web_view(cx, ids!(web_view))
                            .set_scroll_state(cx, self.scroll_y_estimate, self.content_height_estimate, vp_h);
                        handled_input = true;
                    }

                    // ----- Keyboard events -----
                    ServoWebViewAction::KeyDown { key_event } => {
                        if let Some(event) = crate::input::translate_key_event(key_event, true) {
                            self.send_input_event(event);
                            handled_input = true;
                        }
                    }
                    ServoWebViewAction::KeyUp { key_event } => {
                        if let Some(event) = crate::input::translate_key_event(key_event, false) {
                            self.send_input_event(event);
                            handled_input = true;
                        }
                    }

                    // ----- IME / text input -----
                    ServoWebViewAction::TextInput { input } => {
                        if !input.is_empty() {
                            self.send_input_event(servo::InputEvent::Keyboard(
                                KeyboardEvent::from_state_and_key(
                                    KeyState::Down,
                                    Key::Named(NamedKey::Process),
                                ),
                            ));
                            self.send_input_event(servo::InputEvent::Ime(
                                ImeEvent::Composition(CompositionEvent {
                                    state: CompositionState::End,
                                    data: input.clone(),
                                }),
                            ));
                            self.send_input_event(servo::InputEvent::Keyboard(
                                KeyboardEvent::from_state_and_key(
                                    KeyState::Up,
                                    Key::Named(NamedKey::Process),
                                ),
                            ));
                            handled_input = true;
                        }
                    }
                }
            }
        }

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

enum NavCommand {
    Back,
    Forward,
    Reload,
    Navigate(String),
}

impl AppMain for App {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        // Lazy init servo on first event
        self.init_servo(cx);

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


