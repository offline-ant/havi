use crossbeam_channel::Sender;
use euclid::Scale;
use havi_protocols::credentials::global_credential_store;
use makepad_widgets::event::VideoSource as PlatformVideoSource;
use makepad_widgets::makepad_platform::gl_render_bridge::GlApi;
use makepad_widgets::makepad_platform::makepad_micro_serde::DeJson;
use makepad_widgets::makepad_platform::studio::StudioToApp;
use makepad_widgets::*;
use media::controller::{
    self as media_controller, MediaEvent as ThreadMediaEvent, MediaOrigin as ThreadMediaOrigin,
    VideoOp,
};
use servo::protocol_handler::ProtocolRegistry;
use servo::{DeviceIndependentPixel, DevicePixel, WebViewId};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Once;
use std::sync::mpsc;

mod actions;
mod camera;
mod capabilities;
mod clipboard;
mod context_menu;
mod delegate;
mod input_handling;
mod navigation;
mod pylon_menu;
mod runtime;
mod tabs;

use camera::CameraState;
use clipboard::ClipboardState;
use delegate::{HaviServoDelegate, HaviWebViewDelegate, MakepadEventLoopWaker, MakepadServoAction};
use navigation::NavCommand;
use pylon_menu::PylonStatus;
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

                pass.clear_color: vec4(1.0, 1.0, 1.0, 1.0)
                body +: {
                    flow: Overlay
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

                        nav_control := View{
                            width: Fill height: Fit
                            flow: Right
                            spacing: 4
                            show_child_controls: false
                            on_control: {
                                get: |arg| self.url_input.text()
                                set: |arg| {
                                    self.url_input.set_text(arg)
                                    self.url_input.text()
                                }
                                focus: |arg| {
                                    self.url_input.focus()
                                    ""
                                }
                                go: |arg| {
                                    if arg != "" {
                                        self.url_input.set_text(arg)
                                    }
                                    self.go_btn.on_click()
                                    self.url_input.text()
                                }
                                edit: |arg| {
                                    if arg != "" {
                                        self.url_input.set_text(arg)
                                    }
                                    self.edit_btn.on_click()
                                    self.url_input.text()
                                }
                            }

                            url_input := TextInput{
                                width: Fill height: Fit
                                empty_text: "Enter URC..."
                            }

                            go_btn := Button{ text: "🚀" }
                            edit_btn := Button{ text: "✏️" }
                        }
                        watch_control := View{
                            width: Fit height: Fit
                            show_child_controls: false
                            on_control: {
                                get: |arg| {
                                    let text = self.watch_btn.text()
                                    if text == "🔔" {
                                        "notify"
                                    }
                                    else if text == "🔁" {
                                        "auto"
                                    }
                                    else if text == "⚡" {
                                        "dev"
                                    }
                                    else {
                                        "off"
                                    }
                                }
                                next: |arg| {
                                    self.watch_btn.on_click()
                                    ""
                                }
                                set: |arg| {
                                    let text = self.watch_btn.text()
                                    if arg == "notify" {
                                        if text == "👁️" { self.watch_btn.on_click() }
                                        else if text == "🔁" {
                                            self.watch_btn.on_click()
                                            self.watch_btn.on_click()
                                            self.watch_btn.on_click()
                                        }
                                        else if text == "⚡" {
                                            self.watch_btn.on_click()
                                            self.watch_btn.on_click()
                                        }
                                    }
                                    else if arg == "auto" {
                                        if text == "👁️" {
                                            self.watch_btn.on_click()
                                            self.watch_btn.on_click()
                                        }
                                        else if text == "🔔" { self.watch_btn.on_click() }
                                        else if text == "⚡" {
                                            self.watch_btn.on_click()
                                            self.watch_btn.on_click()
                                            self.watch_btn.on_click()
                                        }
                                    }
                                    else if arg == "dev" {
                                        if text == "👁️" {
                                            self.watch_btn.on_click()
                                            self.watch_btn.on_click()
                                            self.watch_btn.on_click()
                                        }
                                        else if text == "🔔" {
                                            self.watch_btn.on_click()
                                            self.watch_btn.on_click()
                                        }
                                        else if text == "🔁" { self.watch_btn.on_click() }
                                    }
                                    else if arg == "off" {
                                        if text == "🔔" {
                                            self.watch_btn.on_click()
                                            self.watch_btn.on_click()
                                            self.watch_btn.on_click()
                                        }
                                        else if text == "🔁" {
                                            self.watch_btn.on_click()
                                            self.watch_btn.on_click()
                                        }
                                        else if text == "⚡" { self.watch_btn.on_click() }
                                    }
                                    arg
                                }
                            }

                            watch_btn := Button{ text: "👁️" }
                        }
                        share_btn := Button{ text: "🔗" }
                        home_btn := Button{ text: "🏠" }
                        dock_control := View{
                            width: Fit height: Fit
                            show_child_controls: false
                            on_control: {
                                get: |arg| {
                                    if self.dock_btn.text() == "🔽" {
                                        "bottom"
                                    }
                                    else {
                                        "top"
                                    }
                                }
                                toggle: |arg| {
                                    self.dock_btn.on_click()
                                    ""
                                }
                                set: |arg| {
                                    let text = self.dock_btn.text()
                                    if arg == "bottom" && text != "🔽" {
                                        self.dock_btn.on_click()
                                    }
                                    else if arg == "top" && text != "🔼" {
                                        self.dock_btn.on_click()
                                    }
                                    arg
                                }
                            }

                            dock_btn := Button{ text: "↕️" }
                        }

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

                        // Context menu rendered in popup window (see context_menu.rs)
                        context_menu := View{
                            visible: false
                            width: 220 height: Fit
                            flow: Down
                            padding: Inset{left: 4 right: 4 top: 4 bottom: 4}
                            spacing: 0
                            show_bg: true
                            draw_bg.color: #xffffff

                            // Extracted once and removed from children at runtime.
                            context_item_template := View{
                                width: 212 height: 28
                                flow: Overlay
                                context_item_button := Button{
                                    text: "Action"
                                    width: Fill height: Fill
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

                            // Extracted once and removed from children at runtime.
                            context_separator_template := View{
                                width: Fill
                                height: 9
                                flow: Overlay
                                sep_line := View{
                                    width: Fill
                                    height: 1
                                    margin: Inset{left: 8 right: 8 top: 4 bottom: 4}
                                    show_bg: true
                                    draw_bg.color: #xe3e3e3
                                }
                            }
                        }
                        // Pylon status dropdown menu
                        pylon_menu := View{
                            visible: false
                            abs_pos: vec2(-1000.0, -1000.0)
                            width: 200 height: Fit
                            flow: Down
                            padding: Inset{left: 6 right: 6 top: 4 bottom: 4}
                            spacing: 1
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
                                draw_text.text_style.font_size: 10.0
                                width: Fill height: Fit
                                margin: Inset{left: 0 right: 0 top: 0 bottom: 2}
                            }

                            pylon_menu_services := Label{
                                text: ""
                                draw_text.color: #x333333
                                draw_text.text_style.font_size: 9.0
                                width: Fill height: Fit
                                margin: Inset{left: 0 right: 0 top: 0 bottom: 2}
                            }

                            pylon_hpprd_start_btn := Button{
                                visible: false
                                text: "Start hpprd"
                                width: Fill height: 22
                                padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 10.0
                                draw_bg +: { color: uniform(#xf5f5f5) color_hover: uniform(#xe8e8e8)
                                    pixel: fn() { let sdf = Sdf2d.viewport(self.pos * self.rect_size) sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y) sdf.fill(mix(self.color, self.color_hover, self.hover)) return sdf.result } }
                            }
                            pylon_hpprd_stop_btn := Button{
                                visible: false
                                text: "Stop hpprd"
                                width: Fill height: 22
                                padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 10.0
                                draw_bg +: { color: uniform(#xf5f5f5) color_hover: uniform(#xe8e8e8)
                                    pixel: fn() { let sdf = Sdf2d.viewport(self.pos * self.rect_size) sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y) sdf.fill(mix(self.color, self.color_hover, self.hover)) return sdf.result } }
                            }
                            pylon_nfs_start_btn := Button{
                                visible: false
                                text: "Start NFS"
                                width: Fill height: 22
                                padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 10.0
                                draw_bg +: { color: uniform(#xf5f5f5) color_hover: uniform(#xe8e8e8)
                                    pixel: fn() { let sdf = Sdf2d.viewport(self.pos * self.rect_size) sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y) sdf.fill(mix(self.color, self.color_hover, self.hover)) return sdf.result } }
                            }
                            pylon_nfs_stop_btn := Button{
                                visible: false
                                text: "Stop NFS"
                                width: Fill height: 22
                                padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 10.0
                                draw_bg +: { color: uniform(#xf5f5f5) color_hover: uniform(#xe8e8e8)
                                    pixel: fn() { let sdf = Sdf2d.viewport(self.pos * self.rect_size) sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y) sdf.fill(mix(self.color, self.color_hover, self.hover)) return sdf.result } }
                            }
                            pylon_mount_btn := Button{
                                visible: false
                                text: "Mount"
                                width: Fill height: 22
                                padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 10.0
                                draw_bg +: { color: uniform(#xf5f5f5) color_hover: uniform(#xe8e8e8)
                                    pixel: fn() { let sdf = Sdf2d.viewport(self.pos * self.rect_size) sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y) sdf.fill(mix(self.color, self.color_hover, self.hover)) return sdf.result } }
                            }
                            pylon_unmount_btn := Button{
                                visible: false
                                text: "Unmount"
                                width: Fill height: 22
                                padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 10.0
                                draw_bg +: { color: uniform(#xf5f5f5) color_hover: uniform(#xe8e8e8)
                                    pixel: fn() { let sdf = Sdf2d.viewport(self.pos * self.rect_size) sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y) sdf.fill(mix(self.color, self.color_hover, self.hover)) return sdf.result } }
                            }
                            pylon_services_btn := Button{
                                text: "Services page"
                                width: Fill height: 22
                                padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
                                margin: Inset{left: 0 right: 0 top: 2 bottom: 0}
                                draw_text.color: #x111111
                                draw_text.text_style.font_size: 10.0
                                draw_bg +: { color: uniform(#xf5f5f5) color_hover: uniform(#xe8e8e8)
                                    pixel: fn() { let sdf = Sdf2d.viewport(self.pos * self.rect_size) sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y) sdf.fill(mix(self.color, self.color_hover, self.hover)) return sdf.result } }
                            }
                            pylon_shutdown_btn := Button{
                                text: "Shutdown"
                                width: Fill height: 22
                                padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
                                draw_text.color: #xcc3333
                                draw_text.text_style.font_size: 10.0
                                draw_bg +: { color: uniform(#xf5f5f5) color_hover: uniform(#xfce8e8)
                                    pixel: fn() { let sdf = Sdf2d.viewport(self.pos * self.rect_size) sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y) sdf.fill(mix(self.color, self.color_hover, self.hover)) return sdf.result } }
                            }
                        }
                    } // end content_area
                    } // end main_layout
                    splash_screen := View{
                        visible: true
                        width: Fill
                        height: Fill
                        flow: Down
                        align: Align{x: 0.5, y: 0.5}
                        show_bg: true
                        draw_bg.color: #xffffff

                        splash_title := Label{
                            text: "HAVI"
                            draw_text.color: #x111111
                            draw_text.text_style.font_size: 48.0
                            align: Align{x: 0.5 y: 0.5}
                        }
                        splash_status := Label{
                            text: "Starting…"
                            draw_text.color: #x999999
                            draw_text.text_style.font_size: 12.0
                            margin: Inset{top: 16}
                            align: Align{x: 0.5 y: 0.5}
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
        havi_render::shaders::script_mod(vm);
        crate::servo_web_view::script_mod(vm);
        App::from_script_mod(vm, self::script_mod)
    }

    pub(super) fn init_media_bridge(&mut self) {
        if self.video_op_rx.is_some() {
            return;
        }

        static INSTALL_MEDIA_PLUGIN: Once = Once::new();
        INSTALL_MEDIA_PLUGIN.call_once(makepad_media::install);

        let (tx, rx) = media_controller::create_video_op_channel();
        media_controller::set_video_op_sender(tx);
        media_controller::set_can_play_type_fn(makepad_widgets::makepad_platform::can_play_type);
        self.video_op_rx = Some(rx);
        log!("[video] media bridge initialized");
    }

    pub(super) fn drain_video_ops(&mut self, cx: &mut Cx) {
        let Some(rx) = self.video_op_rx.as_ref().cloned() else {
            return;
        };

        while let Ok(op) = rx.try_recv() {
            match op {
                VideoOp::PrepareVideo {
                    video_id,
                    source,
                    image_key,
                    autoplay,
                    should_loop,
                } => {
                    let texture = Texture::new_with_format(cx, TextureFormat::VideoExternal);
                    havi_render::video_texture_map::set_external_texture(image_key, texture.clone());
                    self.video_image_keys.insert(video_id, image_key);
                    self.video_logged_first_frame.remove(&video_id);
                    self.video_texture_update_count.remove(&video_id);

                    log!(
                        "[video] prepare id={} key={:?} autoplay={} loop={}",
                        video_id,
                        image_key,
                        autoplay,
                        should_loop
                    );

                    cx.prepare_video_playback(
                        LiveId(video_id),
                        makepad_video_source(source),
                        makepad_widgets::makepad_platform::event::video_playback::CameraPreviewMode::Texture,
                        0,
                        texture.texture_id(),
                        autoplay,
                        should_loop,
                    );
                },
                VideoOp::PrepareAudio {
                    video_id,
                    source,
                    autoplay,
                    should_loop,
                } => {
                    log!(
                        "[video] prepare-audio id={} autoplay={} loop={}",
                        video_id,
                        autoplay,
                        should_loop
                    );
                    cx.prepare_audio_playback(
                        LiveId(video_id),
                        makepad_video_source(source),
                        autoplay,
                        should_loop,
                    );
                },
                VideoOp::Play(video_id) => cx.begin_video_playback(LiveId(video_id)),
                VideoOp::Pause(video_id) => cx.pause_video_playback(LiveId(video_id)),
                VideoOp::Resume(video_id) => cx.resume_video_playback(LiveId(video_id)),
                VideoOp::Mute(video_id) => cx.mute_video_playback(LiveId(video_id)),
                VideoOp::Unmute(video_id) => cx.unmute_video_playback(LiveId(video_id)),
                VideoOp::Seek {
                    video_id,
                    position_ms,
                } => cx.seek_video_playback(LiveId(video_id), position_ms),
                VideoOp::SetVolume { video_id, volume } => {
                    cx.set_video_volume(LiveId(video_id), volume)
                },
                VideoOp::SetPlaybackRate { video_id, rate } => {
                    cx.set_video_playback_rate(LiveId(video_id), rate)
                },
                VideoOp::Cleanup(video_id) => {
                    if let Some(image_key) = self.video_image_keys.remove(&video_id) {
                        havi_render::video_texture_map::remove_video_binding(image_key);
                    }
                    self.video_logged_first_frame.remove(&video_id);
                    self.video_texture_update_count.remove(&video_id);
                    self.mse_players.remove(&video_id);
                    cx.cleanup_video_playback_resources(LiveId(video_id));
                },

                // --- MSE operations ---

                VideoOp::PrepareMseVideo { video_id, mime, image_key } => {
                    log!("[mse] prepare id={} mime={} key={:?}", video_id, mime, image_key);
                    let texture = Texture::new_with_format(cx, TextureFormat::VideoExternal);
                    havi_render::video_texture_map::set_external_texture(image_key, texture);
                    self.video_image_keys.insert(video_id, image_key);

                    match makepad_widgets::makepad_platform::media_plugin()
                        .ok_or_else(|| "no media plugin".to_string())
                        .and_then(|p| p.create_mse_player(&mime))
                    {
                        Ok(player) => {
                            self.mse_players.insert(video_id, player);
                        }
                        Err(e) => {
                            log!("[mse] error creating player: {}", e);
                            media_controller::dispatch_media_event(
                                video_id,
                                ThreadMediaEvent::MseError(e),
                            );
                        }
                    }
                },
                VideoOp::MseAppendData { video_id, data } => {
                    if let Some(player) = self.mse_players.get_mut(&video_id) {
                        match player.append_data(&data) {
                            Ok(result) => {
                                if result.init_segment_parsed {
                                    log!(
                                        "[mse] init parsed id={} {}x{} dur={}ms",
                                        video_id, result.width, result.height, result.duration_ms
                                    );
                                    media_controller::dispatch_media_event(
                                        video_id,
                                        ThreadMediaEvent::MseInitSegmentParsed {
                                            width: result.width,
                                            height: result.height,
                                            duration_ms: result.duration_ms,
                                        },
                                    );
                                }
                                // TODO: upload decoded YUV frames to GPU textures
                                // once platform texture-from-data path is wired.
                                let has_frames = !result.new_frames.is_empty();
                                if has_frames {
                                    log!("[mse] decoded {} frames for id={}", result.new_frames.len(), video_id);
                                    self.needs_paint = true;
                                    self.idle_frames = 0;
                                    self.next_frame = cx.new_next_frame();
                                    cx.redraw_all();
                                }
                                media_controller::dispatch_media_event(
                                    video_id,
                                    ThreadMediaEvent::MseAppendDone {
                                        buffered_ranges: result.buffered_ranges,
                                    },
                                );
                            }
                            Err(e) => {
                                log!("[mse] append error id={}: {}", video_id, e);
                                media_controller::dispatch_media_event(
                                    video_id,
                                    ThreadMediaEvent::MseError(e),
                                );
                            }
                        }
                    }
                },
                VideoOp::MseEndOfStream { video_id } => {
                    if let Some(player) = self.mse_players.get_mut(&video_id) {
                        match player.end_of_stream() {
                            Ok(_frames) => {
                                media_controller::dispatch_media_event(
                                    video_id,
                                    ThreadMediaEvent::PlaybackCompleted,
                                );
                            }
                            Err(e) => {
                                media_controller::dispatch_media_event(
                                    video_id,
                                    ThreadMediaEvent::MseError(e),
                                );
                            }
                        }
                    }
                },
                VideoOp::MseRemove { video_id, start, end } => {
                    if let Some(player) = self.mse_players.get_mut(&video_id) {
                        player.remove(start, end);
                    }
                },
            }
        }
    }

    pub(super) fn handle_video_event(&mut self, cx: &mut Cx, event: &Event) {
        match event {
            Event::VideoPlaybackPrepared(ev) => {
                log!(
                    "[video] prepared id={} {}x{} duration={}ms",
                    ev.video_id.0,
                    ev.video_width,
                    ev.video_height,
                    ev.duration
                );
                media_controller::dispatch_media_event(
                    ev.video_id.0,
                    ThreadMediaEvent::Prepared {
                        width: ev.video_width,
                        height: ev.video_height,
                        duration_ms: ev.duration,
                        is_seekable: ev.is_seekable,
                        video_tracks: ev.video_tracks.clone(),
                        audio_tracks: ev.audio_tracks.clone(),
                    },
                );
            },
            Event::VideoYuvTexturesReady(ev) => {
                let image_key = self
                    .video_image_keys
                    .get(&ev.video_id.0)
                    .copied()
                    .or_else(|| self.camera.image_key_for_video_id(ev.video_id.0));
                if let Some(image_key) = image_key {
                    havi_render::video_texture_map::set_yuv_planes(
                        image_key,
                        ev.tex_y.clone(),
                        ev.tex_u.clone(),
                        ev.tex_v.clone(),
                    );
                }
                self.needs_paint = true;
                self.idle_frames = 0;
                self.next_frame = cx.new_next_frame();
                cx.redraw_all();
            },
            Event::VideoTextureUpdated(ev) => {
                let image_key = self
                    .video_image_keys
                    .get(&ev.video_id.0)
                    .copied()
                    .or_else(|| self.camera.image_key_for_video_id(ev.video_id.0));
                if let Some(image_key) = image_key {
                    havi_render::video_texture_map::set_yuv_metadata(image_key, ev.yuv);
                }

                let count = self
                    .video_texture_update_count
                    .entry(ev.video_id.0)
                    .and_modify(|n| *n += 1)
                    .or_insert(1);

                if self.video_logged_first_frame.insert(ev.video_id.0) {
                    log!(
                        "[video] first-frame id={} pos={}ms",
                        ev.video_id.0,
                        ev.current_position_ms
                    );
                }
                if *count <= 5 || *count % 30 == 0 {
                    log!(
                        "[video] frame id={} count={} pos={}ms",
                        ev.video_id.0,
                        count,
                        ev.current_position_ms
                    );
                }

                media_controller::dispatch_media_event(
                    ev.video_id.0,
                    ThreadMediaEvent::PositionChanged(ev.current_position_ms),
                );

                self.needs_paint = true;
                self.idle_frames = 0;
                self.next_frame = cx.new_next_frame();
                cx.redraw_all();
            },
            Event::VideoPlaybackCompleted(ev) => {
                let frames = self
                    .video_texture_update_count
                    .get(&ev.video_id.0)
                    .copied()
                    .unwrap_or(0);
                log!("[video] completed id={} frames={}", ev.video_id.0, frames);
                media_controller::dispatch_media_event(
                    ev.video_id.0,
                    ThreadMediaEvent::PlaybackCompleted,
                );
            },
            Event::VideoDecodingError(ev) => {
                log!("[video] error id={} {}", ev.video_id.0, ev.error);
                media_controller::dispatch_media_event(
                    ev.video_id.0,
                    ThreadMediaEvent::Error(ev.error.clone()),
                );
            },
            Event::VideoSeekableRanges(ev) => {
                media_controller::dispatch_media_event(
                    ev.video_id.0,
                    ThreadMediaEvent::SeekableRanges(ev.ranges.clone()),
                );
            },
            Event::VideoBufferedRanges(ev) => {
                media_controller::dispatch_media_event(
                    ev.video_id.0,
                    ThreadMediaEvent::BufferedRanges(ev.ranges.clone()),
                );
            },
            Event::VideoPlaybackResourcesReleased(ev) => {
                if let Some(image_key) = self.video_image_keys.remove(&ev.video_id.0) {
                    havi_render::video_texture_map::remove_video_binding(image_key);
                }
                self.video_logged_first_frame.remove(&ev.video_id.0);
                self.video_texture_update_count.remove(&ev.video_id.0);
            },
            Event::VideoInputs(ev) => {
                self.camera.handle_video_inputs_event(ev);
            },
            _ => {},
        }
    }
}

fn makepad_video_source(source: ThreadMediaOrigin) -> PlatformVideoSource {
    match source {
        ThreadMediaOrigin::InMemory(data) => {
            // Fast path: avoid cloning the full in-memory payload when this Arc
            // has unique ownership at the hand-off boundary.
            let bytes = match std::sync::Arc::try_unwrap(data) {
                Ok(bytes) => bytes,
                Err(shared) => shared.as_ref().clone(),
            };
            PlatformVideoSource::InMemory(Rc::new(bytes))
        },
        ThreadMediaOrigin::Network(url) => PlatformVideoSource::Network(url),
        ThreadMediaOrigin::Filesystem(path) => PlatformVideoSource::Filesystem(path),
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[derive(Clone, Copy, Debug)]
pub(super) struct PendingClipboardMenu {
    anchor_abs: DVec2,
    baseline_revision: u64,
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,

    #[rust]
    clipboard_state: Option<Rc<ClipboardState>>,
    #[rust]
    servo: Option<servo::Servo>,
    #[rust]
    rendering_context: Option<Rc<servo::MakepadRenderingContext>>,
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

    // --- Context menu state ---
    /// Active Servo context menu awaiting user selection. Presence means menu is open.
    #[rust]
    active_context_menu: Option<servo::ContextMenu>,
    #[rust]
    context_menu_pos: DVec2,
    /// Popup window handle for the context menu. None when menu is closed.
    #[rust]
    context_popup_window: Option<WindowHandle>,
    /// Draw pass for the context menu popup window.
    #[rust]
    context_popup_pass: Option<DrawPass>,
    /// Draw list for rendering context menu contents into the popup pass.
    #[rust]
    context_popup_draw_list: Option<DrawList2d>,
    /// Dynamic context-menu entries mirroring Servo's menu payload.
    #[rust]
    context_menu_entries: Vec<context_menu::ContextMenuEntry>,
    /// Cached ScriptObjectRef for context item template.
    #[rust]
    context_item_template_source: ScriptObjectRef,
    /// Cached ScriptObjectRef for context separator template.
    #[rust]
    context_separator_template_source: ScriptObjectRef,
    /// Latest known element flags for context-sensitive capabilities.
    #[rust]
    last_context_menu_flags: Option<servo::ContextMenuElementInformationFlags>,

    /// True while Servo reports an active IME/editable context.
    #[rust]
    ime_visible: bool,

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

    /// Receiver for media-thread VideoOp commands (script thread -> makepad main thread).
    #[rust]
    video_op_rx: Option<crossbeam_channel::Receiver<VideoOp>>,

    /// Mapping from media video_id to image key for video texture registration.
    #[rust]
    video_image_keys: HashMap<u64, (u32, u32)>,

    /// Tracks whether a first frame has been observed for each video_id.
    #[rust]
    video_logged_first_frame: HashSet<u64>,

    /// Per-video number of VideoTextureUpdated events seen.
    #[rust]
    video_texture_update_count: HashMap<u64, u64>,

    /// MSE players keyed by video_id.
    #[rust]
    mse_players: HashMap<u64, Box<dyn makepad_widgets::makepad_platform::MsePlayer>>,

    /// Camera subsystem state.
    #[rust]
    camera: CameraState,

    /// Last primary selection text sent to the platform, for change detection.
    #[cfg(target_os = "linux")]
    #[rust]
    last_primary_selection: String,

    /// Whether selection handles are currently shown (mobile).
    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[rust]
    selection_handles_visible: bool,

    /// Pending clipboard-action menu request waiting for fresh selection snapshot.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[rust]
    pending_clipboard_menu: Option<PendingClipboardMenu>,

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

    /// Timer for splash screen timeout (3 seconds max during pylon boot).
    #[rust]
    splash_timeout: Timer,
}

/// Maximum number of idle frames before stopping the frame loop.
/// When the frame loop stops, Servo's `wake()` call will restart it.
const MAX_IDLE_FRAMES: u32 = 10;

/// Distance threshold (in logical pixels) to distinguish taps from scrolls.
/// If the finger moves more than this distance from the initial touch point,
/// the gesture is treated as a scroll; otherwise it's a tap (click).
const TAP_DISTANCE_THRESHOLD: f64 = 5.0;
