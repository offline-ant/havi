use crossbeam_channel::Sender;
use euclid::Scale;
use havi_protocols::credentials::global_credential_store;
use makepad_widgets::makepad_platform::gl_render_bridge::{GlApi, GlRenderBridge};
use makepad_widgets::makepad_platform::makepad_micro_serde::DeJson;
use makepad_widgets::makepad_platform::studio::StudioToApp;
use makepad_widgets::makepad_platform::thread::SignalToUI;
use makepad_widgets::*;
use servo::protocol_handler::ProtocolRegistry;
use servo::{DeviceIndependentPixel, DevicePixel, RenderingContext, WebViewId};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;

mod context_menu;
mod input_handling;
mod navigation;
mod tabs;

use navigation::NavCommand;
use tabs::{HOME_URL, TabInfo, next_tab_live_id, title_from_url};

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
                        watch_btn := Button{ text: "W:Off" }
                        share_btn := Button{ text: "🔗" }
                        home_btn := Button{ text: "⌂" }

                        repo_mode_label := Label{
                            text: ""
                            draw_text.color: #x666666
                            draw_text.text_style.font_size: 9.0
                            width: Fit height: Fit
                            margin: Inset{left: 4 right: 0 top: 0 bottom: 0}
                        }
                        repo_mode_label_off := Label{
                            visible: false
                            text: ""
                            draw_text.color: #xff4444
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

fn mode_from_wire(mode: &str) -> Option<havi_protocols::watch::WatchMode> {
    match mode {
        "off" => Some(havi_protocols::watch::WatchMode::Off),
        "notify" => Some(havi_protocols::watch::WatchMode::Notify),
        "auto" => Some(havi_protocols::watch::WatchMode::Auto),
        "dev" => Some(havi_protocols::watch::WatchMode::Dev),
        _ => None,
    }
}

fn mode_to_wire(mode: havi_protocols::watch::WatchMode) -> String {
    match mode {
        havi_protocols::watch::WatchMode::Off => "off",
        havi_protocols::watch::WatchMode::Notify => "notify",
        havi_protocols::watch::WatchMode::Auto => "auto",
        havi_protocols::watch::WatchMode::Dev => "dev",
    }
    .to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PylonMode {
    None,
    External,
    Embedded,
}

fn pylon_mode_from_env() -> PylonMode {
    match std::env::var("HAVI_PYLON_MODE").ok().as_deref() {
        Some("none") => PylonMode::None,
        Some("external") => PylonMode::External,
        Some("embedded") if cfg!(feature = "embedded-services") => PylonMode::Embedded,
        Some("embedded") => PylonMode::External,
        _ if cfg!(feature = "embedded-services") => PylonMode::Embedded,
        _ => PylonMode::External,
    }
}

fn start_hpprd_with_runtime(
    pylon_client: &mut havi_protocols::pylon::PylonClient,
    runtime: Option<&str>,
) -> anyhow::Result<u16> {
    if let Some(port) = pylon_client.hpprd_port() {
        return Ok(port);
    }

    let mut args = serde_json::Map::new();
    if let Some(mode) = runtime {
        args.insert("runtime".to_string(), serde_json::json!(mode));
    }

    let start_error = pylon_client
        .command("start", Some("hpprd"), Some(&args))
        .err();

    let mut last_state: Option<String> = None;
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(300));
        if let Some(port) = pylon_client.hpprd_port() {
            return Ok(port);
        }

        if let Ok(status) = pylon_client.command("status", None, None) {
            if let Some(hpprd) = status.get("hpprd") {
                let state = hpprd
                    .get("state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let pid = hpprd
                    .get("pid")
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "none".to_string());
                let port = hpprd
                    .get("port")
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "none".to_string());
                let addr = hpprd.get("addr").and_then(|v| v.as_str()).unwrap_or("none");
                last_state = Some(format!(
                    "state={}, pid={}, port={}, addr={}",
                    state, pid, port, addr
                ));
            }
        }
    }

    let mut message = String::from(
        "hpprd did not become reachable after pylon startup request and 10 status polls (~3s).",
    );
    if let Some(e) = start_error {
        message.push_str(" Start request error: ");
        message.push_str(&e);
        message.push('.');
    }
    if let Some(state) = last_state {
        message.push_str(" Last observed hpprd status: ");
        message.push_str(&state);
        message.push('.');
    }

    Err(anyhow::anyhow!(message))
}

#[derive(Clone, Debug)]
pub enum MakepadServoAction {
    None,
    Wake,
    /// A webview's page title changed.
    TitleChanged {
        webview_id: WebViewId,
        title: Option<String>,
    },
    /// A webview's URL changed.
    UrlChanged {
        webview_id: WebViewId,
        url: String,
    },
    /// A webview has new content to paint.
    NewFrameReady {
        webview_id: WebViewId,
    },
    /// A webview was closed by page content (window.close()).
    WebViewClosed {
        webview_id: WebViewId,
    },
    /// Request current watch mode for a specific WebView from app state.
    WatchGetMode {
        webview_id: WebViewId,
        response_sender: Sender<String>,
    },
    /// Set watch mode for a specific WebView in app state and return resulting mode.
    WatchSetMode {
        webview_id: WebViewId,
        mode: String,
        response_sender: Sender<String>,
    },
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
        log!(
            "DEVTOOLS_BIND=127.0.0.1:{} # havi-devtools-cli -p {}",
            port,
            port
        );
    }

    fn request_devtools_connection(&self, request: servo::AllowOrDenyRequest) {
        request.allow();
    }

    fn watch_get_mode(
        &self,
        webview_id: WebViewId,
        response_sender: crossbeam_channel::Sender<String>,
    ) {
        Cx::post_action(MakepadServoAction::WatchGetMode {
            webview_id,
            response_sender,
        });
        SignalToUI::set_ui_signal();
    }

    fn watch_set_mode(
        &self,
        webview_id: WebViewId,
        mode: String,
        response_sender: crossbeam_channel::Sender<String>,
    ) {
        Cx::post_action(MakepadServoAction::WatchSetMode {
            webview_id,
            mode,
            response_sender,
        });
        SignalToUI::set_ui_signal();
    }
}

// ---------------------------------------------------------------------------
// ResourceReader
// ---------------------------------------------------------------------------

struct ResourceReader;

impl servo::resources::ResourceReaderMethods for ResourceReader {
    #[cfg(not(target_os = "android"))]
    fn read(&self, file: servo::resources::Resource) -> Vec<u8> {
        let mut path = std::env::current_exe().unwrap().canonicalize().unwrap();
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
            Resource::DomainList => &include_bytes!("../resources/servo/public_domains.txt")[..],
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
    servo: Option<servo::Servo>,
    #[rust]
    rendering_context: Option<Rc<servo::MakepadRenderingContext>>,
    #[rust]
    bridge: Option<GlRenderBridge>,
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

    // --- Pylon event stream ---
    /// Receives pylon service events. The background reader thread keeps the
    /// TCP connection alive (preventing pylon idle shutdown).
    #[rust]
    pylon_events: Option<std::sync::mpsc::Receiver<havi_protocols::pylon::PylonEvent>>,

    /// Shared HPPR watch connection pool.
    /// Field order matters: this is dropped before `havi_runtime` during App teardown.
    #[rust]
    watch_pool: Option<havi_protocols::watch::WatchPool>,

    /// Dedicated runtime for UI-owned async tasks (watch connections).
    /// Declared after `watch_pool` so watch tasks are aborted before runtime teardown.
    #[rust]
    havi_runtime: Option<tokio::runtime::Runtime>,
}

/// Maximum number of idle frames before stopping the frame loop.
/// When the frame loop stops, Servo's `wake()` call will restart it.
const MAX_IDLE_FRAMES: u32 = 10;

/// Distance threshold (in logical pixels) to distinguish taps from scrolls.
/// If the finger moves more than this distance from the initial touch point,
/// the gesture is treated as a scroll; otherwise it's a tap (click).
const TAP_DISTANCE_THRESHOLD: f64 = 5.0;

/// GL_TEXTURE_RECTANGLE constant (macOS CGL/IOSurface textures).
const GL_TEXTURE_RECTANGLE: u32 = 0x84F5;

/// Build platform display info for WebGL from the GL render bridge.
#[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
fn build_display_info(bridge: &GlRenderBridge) -> servo::gl_device::egl::EglDisplayInfo {
    // Recover the raw eglGetProcAddress function pointer from the bridge.
    // SAFETY: bridge.get_proc_address wraps eglGetProcAddress. Looking up
    // "eglGetProcAddress" returns a pointer to the function itself.
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
fn build_display_info(bridge: &GlRenderBridge) -> servo::gl_device::cgl::CglDisplayInfo {
    servo::gl_device::cgl::CglDisplayInfo {
        pixel_format: bridge.cgl_pixel_format(),
        share_context: bridge.cgl_context(),
    }
}

/// Create the GL render bridge, shared texture, and rendering context.
/// Unified path for all platforms via makepad's GlRenderBridge.
fn create_rendering_context(
    cx: &mut Cx,
    size: dpi::PhysicalSize<u32>,
) -> Result<
    (Texture, GlRenderBridge, Rc<servo::MakepadRenderingContext>),
    servo::rendering_context::Error,
> {
    let bridge = cx.create_gl_render_bridge();
    bridge.make_current();

    let (texture, gl_texture_id) =
        cx.create_gl_render_bridge_texture(&bridge, size.width as usize, size.height as usize);

    let display_info = build_display_info(&bridge);
    let gl_api = match bridge.gl_api() {
        GlApi::GL => servo::gl_device::GlApi::GL,
        GlApi::GLES => servo::gl_device::GlApi::GLES,
    };
    let texture_target = match bridge.gl_api() {
        GlApi::GL => GL_TEXTURE_RECTANGLE,
        GlApi::GLES => gleam::gl::TEXTURE_2D,
    };

    // SAFETY: The bridge's GL context is current (ensured above). GL function
    // pointers loaded via get_proc_address are valid for this context.
    let rc = unsafe {
        servo::MakepadRenderingContext::new_from_loader(
            size,
            &|name| bridge.get_proc_address(name) as *const std::ffi::c_void,
            gl_api,
            texture_target,
            Some(display_info),
        )
    }?;
    rc.set_external_texture(gl_texture_id, size);
    cx.restore_gl_context();

    Ok((texture, bridge, Rc::new(rc)))
}

pub fn install_window_icon() {
    use makepad_widgets::makepad_platform::{set_window_icon, WindowIcon, WindowIconBuffer};

    let png_64 = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources/havi_icon_64.png"));
    let png_128 = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources/havi_icon_128.png"));

    use ::image::codecs::png::PngDecoder;
    use ::image::DynamicImage;

    let dec_64 = match PngDecoder::new(std::io::Cursor::new(&png_64[..])) {
        Ok(dec) => dec,
        Err(err) => {
            eprintln!("[havishell] icon install skipped: invalid 64px png: {}", err);
            return;
        },
    };
    let img_64 = match DynamicImage::from_decoder(dec_64) {
        Ok(img) => img.into_rgba8(),
        Err(err) => {
            eprintln!("[havishell] icon install skipped: decode 64px icon failed: {}", err);
            return;
        },
    };

    let dec_128 = match PngDecoder::new(std::io::Cursor::new(&png_128[..])) {
        Ok(dec) => dec,
        Err(err) => {
            eprintln!("[havishell] icon install skipped: invalid 128px png: {}", err);
            return;
        },
    };
    let img_128 = match DynamicImage::from_decoder(dec_128) {
        Ok(img) => img.into_rgba8(),
        Err(err) => {
            eprintln!("[havishell] icon install skipped: decode 128px icon failed: {}", err);
            return;
        },
    };

    set_window_icon(WindowIcon {
        name: None,
        buffers: vec![
            WindowIconBuffer {
                width: 64,
                height: 64,
                scale: 1,
                data: img_64.into_raw(),
            },
            WindowIconBuffer {
                width: 128,
                height: 128,
                scale: 2,
                data: img_128.into_raw(),
            },
        ],
    });
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

        // Set Wayland app_id to "havi"
        cx.windows[CxWindowPool::id_zero()].create_app_id = "havi".to_string();

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

        // Create rendering context + texture via the unified GL render bridge.
        let size = dpi::PhysicalSize::new(width, height);
        let (texture, bridge, rendering_context) = match create_rendering_context(cx, size) {
            Ok(result) => result,
            Err(e) => {
                log!("[havishell] FAILED to create rendering context: {:?}", e);
                return;
            },
        };
        self.bridge = Some(bridge);

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

        let pylon_mode = pylon_mode_from_env();
        let mut pylon_disabled_reason: Option<String> = None;

        let pylon_port = match pylon_mode {
            PylonMode::None => {
                hppr_client::set_repo_target(fallback_target.clone());
                pylon_disabled_reason = Some("pylon: off (--no-pylon)".to_string());
                None
            }
            PylonMode::External | PylonMode::Embedded => {
                let host_mode = match pylon_mode {
                    PylonMode::External => crate::pylon_host::PylonHostMode::External,
                    PylonMode::Embedded => crate::pylon_host::PylonHostMode::Embedded,
                    PylonMode::None => unreachable!(),
                };

                match crate::pylon_host::ensure_pylon(&repo_path, home.as_deref(), host_mode) {
                    Ok(mut pylon_client) => {
                        let hpprd_runtime = match pylon_mode {
                            PylonMode::Embedded => Some("inline"),
                            PylonMode::External | PylonMode::None => None,
                        };

                        match start_hpprd_with_runtime(&mut pylon_client, hpprd_runtime) {
                            Ok(hpprd_p) => {
                                let pylon_p = pylon_client.port;
                                self.pylon_events = Some(pylon_client.subscribe());
                                hppr_client::set_repo_target(hppr_client::ViaSpec::Net {
                                    host: "127.0.0.1".to_string(),
                                    port: hpprd_p,
                                    scheme: Some(hppr_client::TransportScheme::Tcp),
                                });
                                log!("[havishell] Pylon hpprd on port {}", hpprd_p);
                                Some(pylon_p)
                            }
                            Err(err) => {
                                eprintln!("[havi] pylon unavailable: hpprd could not be started or reached.");
                                eprintln!("[havi] pylon control: 127.0.0.1:{}", pylon_client.port);
                                eprintln!("[havi] repo path: {}", repo_path.display());
                                eprintln!("[havi] detailed cause chain:\n{:#}", err);
                                hppr_client::set_repo_target(fallback_target.clone());
                                pylon_disabled_reason = Some("pylon: off (hpprd start failed)".to_string());
                                None
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("[havi] pylon unavailable: control plane not found/reachable.");
                        eprintln!("[havi] repo path: {}", repo_path.display());
                        if let Some(home_addr) = home.as_deref() {
                            eprintln!("[havi] mode: remote (HAVI_HOME={})", home_addr);
                        } else {
                            eprintln!("[havi] mode: local");
                        }
                        eprintln!("[havi] detailed cause chain:\n{:#}", err);
                        hppr_client::set_repo_target(fallback_target.clone());
                        pylon_disabled_reason = Some("pylon: off (not found)".to_string());
                        None
                    }
                }
            }
        };

        // Create dedicated HAVI runtime and initialize watch pool for live-reload support.
        // This runtime is owned by App and is independent from Servo/Net runtime ownership.
        let watch_runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("havi-watch")
            .build()
            .expect("failed to create HAVI watch runtime");
        self.havi_runtime = Some(watch_runtime);
        let watch_runtime_handle = self
            .havi_runtime
            .as_ref()
            .expect("HAVI watch runtime must exist before watch pool")
            .handle()
            .clone();
        self.watch_pool = Some(havi_protocols::watch::WatchPool::new(
            watch_runtime_handle,
            SignalToUI::set_ui_signal,
        ));

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
            .build();
        servo.set_delegate(Rc::new(HaviServoDelegate));
        servo.setup_logging();

        // Step 4: Create first WebView with proper HiDPI scale factor.
        let start_url_str = std::env::var("HAVI_URL").unwrap_or_else(|_| HOME_URL.to_string());
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
            watch: Default::default(),
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
        self.ui
            .text_input(cx, ids!(url_input))
            .set_text(cx, &start_url_str);

        // Show repo mode indicator
        let repo_mode_text = pylon_disabled_reason
            .clone()
            .unwrap_or_else(|| "pylon".to_string());
        self.set_repo_mode_label(cx, &repo_mode_text, pylon_disabled_reason.is_some());

        // Set window title caption to "havi"
        self.ui
            .widget(cx, ids!(caption_bar.caption_label.label))
            .set_text(cx, "havi");

        // Sync tab bar UI
        self.sync_tab_bar(cx);

        // Hide the Window's built-in caption bar — we use our own tab_bar_wrap
        self.ui.view(cx, ids!(caption_bar)).set_visible(cx, false);

        // Hide macOS traffic light buttons — HAVI uses its own window controls
        cx.push_unique_platform_op(CxOsOp::HideWindowButtons(CxWindowPool::id_zero()));

        // In Makepad Studio's RunView, window control buttons are meaningless —
        // the child process doesn't own a real window.
        if cx.in_makepad_studio {
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

        // Print eval-compatible environment summary
        {
            let repo_dir = havi_protocols::config::repo_dir();
            eprintln!("HPPRD_REPO={}", repo_dir.display());
            if let Some(pp) = pylon_port {
                eprintln!("PYLON=127.0.0.1:{}", pp);
            }
            eprintln!("HAVI_URL={}", start_url_str);
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

        // Create new texture via the bridge and rebind the rendering context.
        if let Some(bridge) = &self.bridge {
            let (texture, gl_texture_id) = cx.create_gl_render_bridge_texture(
                bridge,
                new_width as usize,
                new_height as usize,
            );
            if let Some(rc) = &self.rendering_context {
                rc.set_external_texture(gl_texture_id, phys_size);
                cx.restore_gl_context();
            }
            self.texture = Some(texture);
        }

        // Assign new texture to ServoWebView widget
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

            // Restore Makepad's own GL context as current.
            cx.restore_gl_context();
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

    fn set_repo_mode_label(&self, cx: &mut Cx, text: &str, off: bool) {
        self.ui
            .widget(cx, ids!(repo_mode_label))
            .set_visible(cx, !off);
        self.ui
            .widget(cx, ids!(repo_mode_label_off))
            .set_visible(cx, off);

        if off {
            self.ui
                .label(cx, ids!(repo_mode_label_off))
                .set_text(cx, text);
        } else {
            self.ui.label(cx, ids!(repo_mode_label)).set_text(cx, text);
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
        if self.ui.button(cx, ids!(watch_btn)).clicked(actions) {
            if let Some(tab) = self.tabs.get_mut(self.active_tab_idx) {
                let next = tab.watch.mode().next();
                tab.watch.set_mode(next);
                self.ui
                    .button(cx, ids!(watch_btn))
                    .set_text(cx, next.label());
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
                    let new_mode = if let Some(mode) = mode_from_wire(mode) {
                        if let Some(idx) = tab_idx {
                            if let Some(tab) = self.tabs.get_mut(idx) {
                                tab.watch.set_mode(mode);
                                if idx == self.active_tab_idx {
                                    self.ui
                                        .button(cx, ids!(watch_btn))
                                        .set_text(cx, mode.label());
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
                        },
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

        // Handle next-frame for servo update loop
        if let Some(_ne) = self.next_frame.is_event(event) {
            // Drain pylon events and update toolbar status
            if let Some(ref rx) = self.pylon_events {
                while let Ok(ev) = rx.try_recv() {
                    let label = match (ev.event.as_str(), ev.service.as_deref()) {
                        ("service_started", Some(svc)) => format!("pylon: {} ●", svc),
                        ("service_stopped", Some(svc)) => format!("pylon: {} ○", svc),
                        _ => String::new(),
                    };
                    if !label.is_empty() {
                        self.set_repo_mode_label(cx, &label, false);
                    }
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
                },
                havi_protocols::watch::WatchAction::ChangeDetected => {
                    cx.redraw_all();
                },
                havi_protocols::watch::WatchAction::None => {},
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
                            if *child_id == live_id!(tab_template) {
                                continue;
                            }
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
