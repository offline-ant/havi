use makepad_widgets::*;

#[allow(unused_imports)]
use crate::servo_web_view::ServoWebView;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.ServoWebView

    mod.widgets.HaviTabBar = View {
        flow: Right
        width: Fill height: 32
        draw_bg.color: #xffffff
        show_bg: true
        align: Align{y: 0.5}

        tab_area := View{
            flow: Right
            width: Fill height: Fill
            align: Align{y: 0.5}

            tab_scroll_left_btn := Button{
                visible: false
                text: "◀"
                width: 28 height: Fill
                margin: Inset{left: 2 right: 2 top: 0 bottom: 0}
            }

            tab_bar := View{
                flow: Right
                event_order: Down
                width: Fill height: Fill
                padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
                spacing: 0
                align: Align{y: 0.5}
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
                    width: 150 height: Fill
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
                width: 28 height: Fill
                margin: Inset{left: 2 right: 2 top: 0 bottom: 0}
            }

            new_tab_btn := Button{
                text: "+"
                width: 28 height: Fill
                padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
                margin: Inset{left: 2 right: 2 top: 0 bottom: 0}
                draw_text.color: #x111111
                draw_text.text_style.font_size: 16.0
                draw_bg +: {
                    pixel: fn() { return vec4(0.0, 0.0, 0.0, 0.0) }
                }
            }
        }

        window_controls := View{
            width: Fit height: Fill
            flow: Right
            align: Align{y: 0.5}

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
                    self.nav_go_btn.on_click()
                    self.url_input.text()
                }
            }

            url_input := TextInput{
                width: Fill height: Fit
                empty_text: "Enter URC..."
                padding: Inset{left: 12 right: 12 top: 6 bottom: 6}
                draw_text +: {
                    color: #x000000
                    color_hover: uniform(#x000000)
                    color_focus: uniform(#x000000)
                    color_empty: uniform(#x666666)
                    color_empty_hover: uniform(#x666666)
                    color_empty_focus: uniform(#x444444)
                }
                draw_bg +: {
                    color: #xffffff
                    color_hover: uniform(#xffffff)
                    color_focus: uniform(#xffffff)
                    border_color: uniform(#xcccccc)
                    border_color_hover: uniform(#x999999)
                    border_color_focus: uniform(#x777777)
                }
                draw_cursor +: {
                    color: uniform(#x000000)
                }
            }

            nav_go_btn := Button{
                text: ""
                width: 0 height: 0
                margin: Inset{left: 0 right: 0 top: 0 bottom: 0}
                padding: Inset{left: 0 right: 0 top: 0 bottom: 0}
                draw_bg +: { pixel: fn() { return vec4(0.0, 0.0, 0.0, 0.0) } }
            }
        }

        info_btn := Button{ text: "i" }
        reload_btn := Button{ text: "🔄" }
        overflow_btn := Button{ text: "⋯" }
    }

    mod.widgets.HaviInfoPanel = View {
        visible: false
        width: 400 height: Fill
        flow: Down
        align: Align{x: 1.0}
        padding: Inset{left: 12 right: 12 top: 12 bottom: 12}
        spacing: 8
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

        panel_title := Label{ text: "Page info" draw_text.color: #x111111 draw_text.text_style.font_size: 12.0 }
        info_scroll := View{
            width: Fill height: Fill
            flow: Down
            spacing: 8
            scroll_bars: ScrollBars{show_scroll_x: false show_scroll_y: true}

            page_section := Label{ text: "" width: Fill draw_text.color: #x222222 draw_text.text_style.font_size: 10.0 }
            source_section := Label{ text: "" width: Fill draw_text.color: #x222222 draw_text.text_style.font_size: 10.0 }
            packet_section := Label{ text: "" width: Fill draw_text.color: #x222222 draw_text.text_style.font_size: 10.0 }
            trace_section := Label{ text: "" width: Fill draw_text.color: #x222222 draw_text.text_style.font_size: 10.0 }
        }

        info_actions := View{
            width: Fill height: Fit
            flow: Right
            spacing: 6
            copy_trace_btn := Button{ text: "Copy lookup trace" }
            open_diagnostics_btn := Button{ text: "Open diagnostics" }
            open_target_btn := Button{ text: "Open final target" }
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

    mod.widgets.HaviOverflowMenu = View {
        visible: false
        width: 220 height: Fit
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

        share_btn := Button{
            text: "Copy share link"
            width: Fill height: 24
            padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
            draw_text.color: #x111111
            draw_text.text_style.font_size: 10.0
            draw_bg +: { color: uniform(#xf5f5f5) color_hover: uniform(#xe8e8e8)
                pixel: fn() { let sdf = Sdf2d.viewport(self.pos * self.rect_size) sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y) sdf.fill(mix(self.color, self.color_hover, self.hover)) return sdf.result } }
        }
        home_btn := Button{
            text: "Open home"
            width: Fill height: 24
            padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
            draw_text.color: #x111111
            draw_text.text_style.font_size: 10.0
            draw_bg +: { color: uniform(#xf5f5f5) color_hover: uniform(#xe8e8e8)
                pixel: fn() { let sdf = Sdf2d.viewport(self.pos * self.rect_size) sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y) sdf.fill(mix(self.color, self.color_hover, self.hover)) return sdf.result } }
        }
        sep_primary := View{
            width: Fill height: 7
            flow: Overlay
            sep_line := View{
                width: Fill
                height: 1
                margin: Inset{left: 4 right: 4 top: 3 bottom: 3}
                show_bg: true
                draw_bg.color: #xe3e3e3
            }
        }
        watch_control := View{
            width: Fill height: Fit
            show_child_controls: false
            on_control: {
                get: |arg| {
                    let text = self.watch_btn.text()
                    if text == "Watch: Page" {
                        "page"
                    }
                    else if text == "Watch: App" {
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
                        if text == "Watch: Off" { self.watch_btn.on_click() }
                        else if text == "Watch: App" {
                            self.watch_btn.on_click()
                            self.watch_btn.on_click()
                        }
                    }
                    else if arg == "app" || arg == "tree" || arg == "dev" {
                        if text == "Watch: Off" {
                            self.watch_btn.on_click()
                            self.watch_btn.on_click()
                        }
                        else if text == "Watch: Page" { self.watch_btn.on_click() }
                    }
                    else if arg == "none" || arg == "off" {
                        if text == "Watch: Page" {
                            self.watch_btn.on_click()
                            self.watch_btn.on_click()
                        }
                        else if text == "Watch: App" { self.watch_btn.on_click() }
                    }
                    arg
                }
            }

            watch_btn := Button{
                text: "Watch: Off"
                width: Fill height: 24
                padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
                draw_text.color: #x111111
                draw_text.text_style.font_size: 10.0
                draw_bg +: { color: uniform(#xf5f5f5) color_hover: uniform(#xe8e8e8)
                    pixel: fn() { let sdf = Sdf2d.viewport(self.pos * self.rect_size) sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y) sdf.fill(mix(self.color, self.color_hover, self.hover)) return sdf.result } }
            }
        }
        shadow_control := View{
            width: Fill height: Fit
            show_child_controls: false
            on_control: {
                get: |arg| {
                    if self.shadow_btn.text() == "Shadow: On" { "on" } else { "off" }
                }
                enter: |arg| {
                    if self.shadow_btn.text() != "Shadow: On" { self.shadow_btn.on_click() }
                    "on"
                }
                exit: |arg| {
                    if self.shadow_btn.text() == "Shadow: On" { self.shadow_btn.on_click() }
                    "off"
                }
                set: |arg| {
                    if (arg == "on" || arg == "enter") && self.shadow_btn.text() != "Shadow: On" {
                        self.shadow_btn.on_click()
                    }
                    else if (arg == "off" || arg == "exit") && self.shadow_btn.text() == "Shadow: On" {
                        self.shadow_btn.on_click()
                    }
                    arg
                }
            }

            shadow_btn := Button{
                text: "Shadow: Off"
                width: Fill height: 24
                padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
                draw_text.color: #x111111
                draw_text.text_style.font_size: 10.0
                draw_bg +: { color: uniform(#xf5f5f5) color_hover: uniform(#xe8e8e8)
                    pixel: fn() { let sdf = Sdf2d.viewport(self.pos * self.rect_size) sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y) sdf.fill(mix(self.color, self.color_hover, self.hover)) return sdf.result } }
            }
        }
        dock_control := View{
            width: Fill height: Fit
            show_child_controls: false
            on_control: {
                get: |arg| {
                    if self.dock_btn.text() == "Move controls to top" {
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
                    if arg == "bottom" && text != "Move controls to top" {
                        self.dock_btn.on_click()
                    }
                    else if arg == "top" && text != "Move controls to bottom" {
                        self.dock_btn.on_click()
                    }
                    arg
                }
            }

            dock_btn := Button{
                text: "Move controls to bottom"
                width: Fill height: 24
                padding: Inset{left: 6 right: 6 top: 2 bottom: 2}
                draw_text.color: #x111111
                draw_text.text_style.font_size: 10.0
                draw_bg +: { color: uniform(#xf5f5f5) color_hover: uniform(#xe8e8e8)
                    pixel: fn() { let sdf = Sdf2d.viewport(self.pos * self.rect_size) sdf.rect(0.0 0.0 self.rect_size.x self.rect_size.y) sdf.fill(mix(self.color, self.color_hover, self.hover)) return sdf.result } }
            }
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
            show_caption_bar: false
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

                        web_view := ServoWebView{
                            width: Fill
                            height: Fill
                        }
                        context_menu := mod.widgets.HaviContextMenu {}
                        overflow_menu := mod.widgets.HaviOverflowMenu {}
                        info_panel := mod.widgets.HaviInfoPanel {}
                    }
                }
                splash_screen := mod.widgets.HaviSplash {}
            }
        }
    }
}
