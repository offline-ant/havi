use crossbeam_channel::Sender;
use euclid::Scale;
use havi_protocols::credentials::global_credential_store;
use makepad_widgets::makepad_platform::gl_render_bridge::{GlApi, GlRenderBridge};
use makepad_widgets::makepad_platform::makepad_micro_serde::DeJson;
use makepad_widgets::makepad_platform::studio::StudioToApp;
use makepad_widgets::*;
use servo::protocol_handler::ProtocolRegistry;
use servo::{DeviceIndependentPixel, DevicePixel, RenderingContext, WebViewId};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;

mod actions;
mod context_menu;
mod delegate;
mod input_handling;
mod navigation;
mod pylon_menu;
mod runtime;
mod tabs;

use delegate::{HaviServoDelegate, HaviWebViewDelegate, MakepadEventLoopWaker, MakepadServoAction};
use navigation::NavCommand;
use pylon_menu::PylonStatus;
use tabs::{HOME_URL, LOADING_URL, TabInfo, next_tab_live_id, title_from_url};


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

                pass.clear_color: vec4(1.0, 1.0, 1.0, 1.0)
                body +: {
                    main_layout := View{
                        width: Fill
                        height: Fill
                        flow: Down

                    // --- Tab bar ---
                    tab_bar_wrap := View{
                        flow: Right
                        width: Fill height: Fit
                        draw_bg.color: #xffffff
                        show_bg: true
                        align: Align{y: 1.0}

                        // Tabs area: fills remaining space after window controls
                        tab_area := View{
                            flow: Right
                            width: Fill height: Fit
                            align: Align{y: 1.0}

                            tab_scroll_left_btn := Button{
                                visible: false
                                text: "◀"
                                width: 28 height: 28
                                margin: Inset{left: 2 right: 2 top: 2 bottom: 2}
                            }

                            tab_bar := View{
                                flow: Right
                                event_order: Down
                                width: Fill height: Fit
                                padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
                                spacing: 0
                                align: Align{y: 1.0}
                                scroll_bars: ScrollBarsTabs{
                                    show_scroll_x: true
                                    show_scroll_y: false
                                    scroll_bar_x +: {
                                        bar_size: 4.0
                                        use_vertical_finger_scroll: true
                                    }
                                }

                                // Template tab — extracted once by sync_tab_bar, then
                                // removed from children. Never kept as a hidden child.
                                tab_template := View{
                                    cursor: MouseCursor.Hand
                                    flow: Right
                                    width: 150 height: Fit
                                    padding: Inset{left: 10 right: 4 top: 5 bottom: 5}
                                    spacing: 6
                                    align: Align{y: 0.5}
                                    show_bg: true
                                    draw_bg +: {
                                        color: uniform(#xffffff)
                                        border_color: uniform(#xcccccc)
                                        pixel: fn() {
                                            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                            sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                            sdf.fill(self.color)
                                            sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                            sdf.stroke(self.border_color, 1.0)
                                            return sdf.result
                                        }
                                    }
                                    tab_label := Label{
                                        text: "New Tab"
                                        draw_text.color: #x111111
                                        draw_text.text_style.font_size: 11.0
                                    }
                                    tab_spacer := View{
                                        width: Fill height: 1
                                    }
                                    tab_close := Label{
                                        text: "×"
                                        draw_text.color: #x999999
                                        draw_text.text_style.font_size: 13.0
                                        width: 20 height: 20
                                        align: Align{x: 0.5 y: 0.5}
                                    }
                                }

                            }

                            tab_scroll_right_btn := Button{
                                visible: false
                                text: "▶"
                                width: 28 height: 28
                                margin: Inset{left: 2 right: 2 top: 2 bottom: 2}
                            }

                            new_tab_btn := Button{
                                text: "+"
                                width: 28 height: 28
                                padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
                                margin: Inset{left: 2 right: 2 top: 2 bottom: 2}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 16.0
                                draw_bg +: {
                                    pixel: fn() { return vec4(0.0, 0.0, 0.0, 0.0) }
                                }
                            }
                        }

                        // Window control buttons — pinned to top-right
                        window_controls := View{
                            width: Fit height: 32
                            flow: Right
                            align: Align{y: 0.0}

                            win_min := Button{
                                text: "—"
                                width: 46 height: 32
                                padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
                                draw_text.color: #x555555
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
                                draw_text.color: #x555555
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
                                draw_text.color: #x555555
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
                        draw_bg.color: #xf5f5f5
                        show_bg: true

                        back_btn := Button{ text: "◀" }
                        forward_btn := Button{ text: "▶" }
                        reload_btn := Button{ text: "🔄" }

                        url_input := TextInput{
                            width: Fill height: Fit
                            empty_text: "Enter URC..."
                        }

                        go_btn := Button{ text: "🚀" }
                        edit_btn := Button{ text: "✏️" }
                        watch_btn := Button{ text: "👁️" }
                        share_btn := Button{ text: "🔗" }
                        home_btn := Button{ text: "🏠" }
                        dock_btn := Button{ text: "↕️" }

                        pylon_dot := View{
                            cursor: MouseCursor.Hand
                            width: 16 height: 16
                            margin: Inset{left: 4 right: 0 top: 0 bottom: 0}
                            show_bg: true
                            draw_bg +: {
                                color: uniform(#x888888)
                                pixel: fn() {
                                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                    let r = min(self.rect_size.x, self.rect_size.y) * 0.4
                                    sdf.circle(self.rect_size.x * 0.5, self.rect_size.y * 0.5, r)
                                    sdf.fill(self.color)
                                    return sdf.result
                                }
                            }
                        }
                    }

                    content_area := View{
                        width: Fill height: Fill
                        flow: Overlay

                        web_view := ServoWebView{
                            width: Fill
                            height: Fill
                        }

                        loading_overlay := View{
                            visible: false
                            width: Fill
                            height: Fill
                            show_bg: true
                            draw_bg.color: #x00000022
                            align: Align{x: 0.5, y: 0.5}

                            loading_label := Label{
                                text: "Starting pylon…"
                                draw_text.color: #x333333
                                draw_text.text_style.font_size: 12.0
                            }
                        }

                        // Context menu overlay (starts off-screen; show_context_menu positions it)
                        context_menu := View{
                            visible: false
                            abs_pos: vec2(-1000.0, -1000.0)
                            width: Fit height: Fit
                            flow: Down
                            padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
                            spacing: 0
                            show_bg: true
                            draw_bg.color: #xffffff

                            context_copy_btn := Button{
                                text: "Copy"
                                width: 160 height: 28
                                padding: Inset{left: 12 right: 12 top: 4 bottom: 4}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 12.0
                                draw_bg +: {
                                    color: uniform(#xffffff)
                                    color_hover: uniform(#xf0f0f0)
                                    pixel: fn() {
                                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                        sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                        sdf.fill(mix(self.color, self.color_hover, self.hover))
                                        return sdf.result
                                    }
                                }
                            }
                            context_edit_btn := Button{
                                text: "Go to Editor"
                                width: 160 height: 28
                                padding: Inset{left: 12 right: 12 top: 4 bottom: 4}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 12.0
                                draw_bg +: {
                                    color: uniform(#xffffff)
                                    color_hover: uniform(#xf0f0f0)
                                    pixel: fn() {
                                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                        sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                        sdf.fill(mix(self.color, self.color_hover, self.hover))
                                        return sdf.result
                                    }
                                }
                            }
                        }
                        // Pylon status dropdown menu
                        pylon_menu := View{
                            visible: false
                            abs_pos: vec2(-1000.0, -1000.0)
                            width: 240 height: Fit
                            flow: Down
                            padding: Inset{left: 8 right: 8 top: 6 bottom: 6}
                            spacing: 2
                            show_bg: true
                            draw_bg +: {
                                color: uniform(#xffffff)
                                border_color: uniform(#xcccccc)
                                pixel: fn() {
                                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                    sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                    sdf.fill(self.color)
                                    sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                    sdf.stroke(self.border_color, 1.0)
                                    return sdf.result
                                }
                            }

                            pylon_menu_header := Label{
                                text: "Pylon"
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 11.0
                                width: Fill height: Fit
                                margin: Inset{left: 0 right: 0 top: 0 bottom: 4}
                            }

                            pylon_menu_services := Label{
                                text: ""
                                draw_text.color: #x333333
                                draw_text.text_style.font_size: 10.0
                                width: Fill height: Fit
                                margin: Inset{left: 0 right: 0 top: 0 bottom: 4}
                            }

                            pylon_hpprd_start_btn := Button{
                                visible: false
                                text: "Start hpprd"
                                width: Fill height: 28
                                padding: Inset{left: 8 right: 8 top: 4 bottom: 4}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 11.0
                                draw_bg +: {
                                    color: uniform(#xf5f5f5)
                                    color_hover: uniform(#xe8e8e8)
                                    pixel: fn() {
                                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                        sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                        sdf.fill(mix(self.color, self.color_hover, self.hover))
                                        return sdf.result
                                    }
                                }
                            }
                            pylon_hpprd_stop_btn := Button{
                                visible: false
                                text: "Stop hpprd"
                                width: Fill height: 28
                                padding: Inset{left: 8 right: 8 top: 4 bottom: 4}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 11.0
                                draw_bg +: {
                                    color: uniform(#xf5f5f5)
                                    color_hover: uniform(#xe8e8e8)
                                    pixel: fn() {
                                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                        sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                        sdf.fill(mix(self.color, self.color_hover, self.hover))
                                        return sdf.result
                                    }
                                }
                            }

                            pylon_nfs_start_btn := Button{
                                visible: false
                                text: "Start NFS"
                                width: Fill height: 28
                                padding: Inset{left: 8 right: 8 top: 4 bottom: 4}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 11.0
                                draw_bg +: {
                                    color: uniform(#xf5f5f5)
                                    color_hover: uniform(#xe8e8e8)
                                    pixel: fn() {
                                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                        sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                        sdf.fill(mix(self.color, self.color_hover, self.hover))
                                        return sdf.result
                                    }
                                }
                            }
                            pylon_nfs_stop_btn := Button{
                                visible: false
                                text: "Stop NFS"
                                width: Fill height: 28
                                padding: Inset{left: 8 right: 8 top: 4 bottom: 4}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 11.0
                                draw_bg +: {
                                    color: uniform(#xf5f5f5)
                                    color_hover: uniform(#xe8e8e8)
                                    pixel: fn() {
                                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                        sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                        sdf.fill(mix(self.color, self.color_hover, self.hover))
                                        return sdf.result
                                    }
                                }
                            }

                            pylon_mount_btn := Button{
                                visible: false
                                text: "Mount"
                                width: Fill height: 28
                                padding: Inset{left: 8 right: 8 top: 4 bottom: 4}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 11.0
                                draw_bg +: {
                                    color: uniform(#xf5f5f5)
                                    color_hover: uniform(#xe8e8e8)
                                    pixel: fn() {
                                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                        sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                        sdf.fill(mix(self.color, self.color_hover, self.hover))
                                        return sdf.result
                                    }
                                }
                            }
                            pylon_unmount_btn := Button{
                                visible: false
                                text: "Unmount"
                                width: Fill height: 28
                                padding: Inset{left: 8 right: 8 top: 4 bottom: 4}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 11.0
                                draw_bg +: {
                                    color: uniform(#xf5f5f5)
                                    color_hover: uniform(#xe8e8e8)
                                    pixel: fn() {
                                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                        sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                        sdf.fill(mix(self.color, self.color_hover, self.hover))
                                        return sdf.result
                                    }
                                }
                            }

                            pylon_shutdown_btn := Button{
                                text: "Shutdown pylon"
                                width: Fill height: 28
                                padding: Inset{left: 8 right: 8 top: 4 bottom: 4}
                                draw_text.color: #xcc3333
                                draw_text.text_style.font_size: 11.0
                                draw_bg +: {
                                    color: uniform(#xf5f5f5)
                                    color_hover: uniform(#xfce8e8)
                                    pixel: fn() {
                                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                        sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y)
                                        sdf.fill(mix(self.color, self.color_hover, self.hover))
                                        return sdf.result
                                    }
                                }
                            }
                        }
                    } // end content_area
                    } // end main_layout
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

fn watch_button_text(mode: havi_protocols::watch::WatchMode) -> &'static str {
    match mode {
        havi_protocols::watch::WatchMode::Off => "👁️",
        havi_protocols::watch::WatchMode::Notify => "🔔",
        havi_protocols::watch::WatchMode::Auto => "🔁",
        havi_protocols::watch::WatchMode::Dev => "⚡",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PylonMode {
    None,
    External,
    Embedded,
}

/// Result of background pylon + hpprd + credential bootstrap.
enum PylonInitResult {
    Ready {
        hpprd_port: u16,
        pylon_port: u16,
        pylon_events: std::sync::mpsc::Receiver<havi_protocols::pylon::PylonEvent>,
    },
    Failed {
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum StartupState {
    #[default]
    Booting,
    Ready,
    Failed,
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
    for _ in 0..20 {
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

        std::thread::sleep(std::time::Duration::from_millis(150));
    }

    let mut message = String::from(
        "hpprd did not become reachable after pylon startup request and 20 status polls (~3s).",
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

// ---------------------------------------------------------------------------
// ResourceReader
// ---------------------------------------------------------------------------

struct ResourceReader;

impl servo::resources::ResourceReaderMethods for ResourceReader {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
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

    #[cfg(any(target_os = "android", target_os = "ios"))]
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

    /// Latest advertised public via from pylon listener events.
    #[rust]
    shared_public_via: Option<String>,

    /// Pylon aggregate status for the status dot and dropdown menu.
    #[rust]
    pylon_status: PylonStatus,

    /// Whether the pylon dropdown menu is open.
    #[rust]
    pylon_menu_open: bool,

    /// Second pylon TCP connection for sending commands (start/stop/mount).
    /// The first connection is consumed by `subscribe()` for event streaming.
    #[rust]
    pylon_command_client: Option<havi_protocols::pylon::PylonClient>,

    /// True when tab/toolbar chrome is docked to the bottom.
    #[rust]
    menu_at_bottom: bool,

    #[rust]
    tab_scroll_x: f64,

    // --- Tab state ---
    #[rust]
    tabs: Vec<TabInfo>,
    #[rust]
    active_tab_idx: usize,
    /// Cached ScriptObjectRef for the tab_template View. Extracted once from
    /// tab_bar children so the template widget is never kept as a hidden child
    /// (which caused ghost DrawQuad rendering artifacts on Linux/OpenGL).
    #[rust]
    tab_template_source: ScriptObjectRef,

    // --- IPC single-instance listener ---
    #[rust]
    ipc_rx: Option<std::sync::mpsc::Receiver<havi_protocols::instance::IpcCommand>>,

    // --- Pylon event stream ---
    /// Receives pylon service events. The background reader thread keeps the
    /// TCP connection alive (preventing pylon idle shutdown).
    #[rust]
    pylon_events: Option<std::sync::mpsc::Receiver<havi_protocols::pylon::PylonEvent>>,

    /// Canonical startup URL selected once during init.
    #[rust]
    start_url: String,

    /// True once startup navigation has been issued.
    #[rust]
    start_navigation_done: bool,

    /// Startup state machine.
    #[rust]
    startup_state: StartupState,

    /// Receives the pylon init result from the background thread.
    #[rust]
    pylon_init_rx: Option<std::sync::mpsc::Receiver<PylonInitResult>>,

    /// Shared HPPR watch connection pool.
    /// Field order matters: this is dropped before `havi_runtime` during App teardown.
    #[rust]
    watch_pool: Option<havi_protocols::watch::WatchPool>,

    /// Endpoint used when initializing watch pool lazily.
    #[rust]
    watch_fallback_endpoint: String,

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

