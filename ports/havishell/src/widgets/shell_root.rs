use makepad_widgets::*;

#[allow(unused_imports)]
use crate::servo_web_view::ServoWebView;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.CachedView
    use mod.widgets.ServoWebView

    mod.widgets.HaviTabBar = View {
        flow: Right
        width: Fill height: Fit
        draw_bg.color: #xffffff
        show_bg: true
        align: Align{y: 1.0}

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

    mod.widgets.HaviToolbar = View {
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
                draw_text +: {
                    color: #x111111
                    color_hover: uniform(#x111111)
                    color_focus: uniform(#x111111)
                    color_empty: uniform(#x777777)
                    color_empty_hover: uniform(#x777777)
                    color_empty_focus: uniform(#x555555)
                }
                draw_bg +: {
                    color: #xffffff
                    color_hover: uniform(#xffffff)
                    color_focus: uniform(#xffffff)
                    border_color: uniform(#xcccccc)
                    border_color_hover: uniform(#xbbbbbb)
                    border_color_focus: uniform(#xaaaaaa)
                }
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
                        "page"
                    }
                    else if text == "⚡" {
                        "app"
                    }
                    else {
                        "none"
                    }
                }
                next: |arg| {
                    self.watch_btn.on_click()
                    ""
                }
                set: |arg| {
                    let text = self.watch_btn.text()
                    if arg == "page" || arg == "notify" || arg == "auto" {
                        if text == "👁️" { self.watch_btn.on_click() }
                        else if text == "⚡" {
                            self.watch_btn.on_click()
                            self.watch_btn.on_click()
                        }
                    }
                    else if arg == "app" || arg == "tree" || arg == "dev" {
                        if text == "👁️" {
                            self.watch_btn.on_click()
                            self.watch_btn.on_click()
                        }
                        else if text == "🔔" { self.watch_btn.on_click() }
                    }
                    else if arg == "none" || arg == "off" {
                        if text == "🔔" {
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
        shadow_control := View{
            width: Fit height: Fit
            show_child_controls: false
            on_control: {
                get: |arg| {
                    if self.shadow_btn.text() == "S:On" { "on" } else { "off" }
                }
                enter: |arg| {
                    if self.shadow_btn.text() != "S:On" { self.shadow_btn.on_click() }
                    "on"
                }
                exit: |arg| {
                    if self.shadow_btn.text() == "S:On" { self.shadow_btn.on_click() }
                    "off"
                }
                set: |arg| {
                    if (arg == "on" || arg == "enter") && self.shadow_btn.text() != "S:On" {
                        self.shadow_btn.on_click()
                    }
                    else if (arg == "off" || arg == "exit") && self.shadow_btn.text() == "S:On" {
                        self.shadow_btn.on_click()
                    }
                    arg
                }
            }

            shadow_btn := Button{ text: "S:Off" }
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
            flow: Overlay

            pylon_dot_circle := View{
                width: Fill height: Fill
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

            pylon_dot_triangle := View{
                visible: false
                width: Fill height: Fill
                show_bg: true
                draw_bg +: {
                    color: uniform(#x888888)
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        sdf.move_to(self.rect_size.x * 0.5, self.rect_size.y * 0.15)
                        sdf.line_to(self.rect_size.x * 0.85, self.rect_size.y * 0.82)
                        sdf.line_to(self.rect_size.x * 0.15, self.rect_size.y * 0.82)
                        sdf.close_path()
                        sdf.fill(self.color)
                        return sdf.result
                    }
                }
            }

            pylon_dot_square := View{
                visible: false
                width: Fill height: Fill
                show_bg: true
                draw_bg +: {
                    color: uniform(#x888888)
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        let inset = min(self.rect_size.x, self.rect_size.y) * 0.18
                        sdf.rect(inset, inset, self.rect_size.x - inset * 2.0, self.rect_size.y - inset * 2.0)
                        sdf.fill(self.color)
                        return sdf.result
                    }
                }
            }

            pylon_dot_diamond := View{
                visible: false
                width: Fill height: Fill
                show_bg: true
                draw_bg +: {
                    color: uniform(#x888888)
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        sdf.move_to(self.rect_size.x * 0.5, self.rect_size.y * 0.12)
                        sdf.line_to(self.rect_size.x * 0.88, self.rect_size.y * 0.5)
                        sdf.line_to(self.rect_size.x * 0.5, self.rect_size.y * 0.88)
                        sdf.line_to(self.rect_size.x * 0.12, self.rect_size.y * 0.5)
                        sdf.close_path()
                        sdf.fill(self.color)
                        return sdf.result
                    }
                }
            }
        }
    }

    mod.widgets.WebViewHost = CachedView {
        width: Fill
        height: Fill
        web_view := ServoWebView{
            width: Fill
            height: Fill
        }
    }

    mod.widgets.HaviContextMenu = View {
        visible: false
        width: 220 height: Fit
        flow: Down
        padding: Inset{left: 4 right: 4 top: 4 bottom: 4}
        spacing: 0
        show_bg: true
        draw_bg.color: #xffffff

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

    mod.widgets.HaviPylonMenu = View {
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

    mod.widgets.HaviSplash = View {
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

    mod.widgets.HaviShellRoot = Root {
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

                    tab_bar_wrap := mod.widgets.HaviTabBar {}
                    toolbar := mod.widgets.HaviToolbar {}

                    content_area := View{
                        width: Fill height: Fill
                        flow: Overlay

                        web_view_texture := mod.widgets.WebViewHost {}
                        context_menu := mod.widgets.HaviContextMenu {}
                        pylon_menu := mod.widgets.HaviPylonMenu {}
                    }
                }
                splash_screen := mod.widgets.HaviSplash {}
            }
        }
    }
}
