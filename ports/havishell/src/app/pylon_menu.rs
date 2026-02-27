use makepad_widgets::*;

use super::App;

/// Aggregate pylon health.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PylonHealth {
    #[default]
    Booting,
    Green,
    Orange,
    Red,
}

/// Per-service snapshot.
#[derive(Clone, Debug)]
pub(super) struct PylonServiceInfo {
    pub name: String,
    pub state: String,
    pub pid: Option<u32>,
    pub port: Option<u16>,
}

/// Mount snapshot.
#[derive(Clone, Debug)]
pub(super) struct PylonMountInfo {
    pub mountpoint: String,
    pub fstype: String,
}

/// Full pylon status snapshot.
#[derive(Clone, Debug, Default)]
pub(super) struct PylonStatus {
    pub health: PylonHealth,
    pub mode: String,
    pub services: Vec<PylonServiceInfo>,
    pub mounts: Vec<PylonMountInfo>,
}

impl PylonStatus {
    /// Parse from a pylon `status` JSON response.
    pub fn from_json(data: &serde_json::Value) -> Self {
        let mode = data
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Non-service top-level keys.
        const SKIP: &[&str] = &["mode", "mounts", "user"];

        let mut services = Vec::new();
        if let Some(obj) = data.as_object() {
            for (key, val) in obj {
                if SKIP.contains(&key.as_str()) {
                    continue;
                }
                if !val.is_object() {
                    continue;
                }
                services.push(PylonServiceInfo {
                    name: key.to_string(),
                    state: val
                        .get("state")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    pid: val.get("pid").and_then(|v| v.as_u64()).map(|n| n as u32),
                    port: val.get("port").and_then(|v| v.as_u64()).map(|n| n as u16),
                });
            }
        }
        services.sort_by(|a, b| a.name.cmp(&b.name));

        let mut mounts = Vec::new();
        if let Some(arr) = data.get("mounts").and_then(|v| v.as_array()) {
            for m in arr {
                mounts.push(PylonMountInfo {
                    mountpoint: m
                        .get("mountpoint")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    fstype: m
                        .get("fstype")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
            }
        }

        let health = compute_health(&services);

        PylonStatus {
            health,
            mode,
            services,
            mounts,
        }
    }

    /// Update a single service state from a pylon event.
    pub fn apply_event(&mut self, service: &str, new_state: &str, pid: Option<u32>, port: Option<u16>) {
        if let Some(svc) = self.services.iter_mut().find(|s| s.name == service) {
            svc.state = new_state.to_string();
            svc.pid = pid;
            if port.is_some() {
                svc.port = port;
            }
        } else {
            self.services.push(PylonServiceInfo {
                name: service.to_string(),
                state: new_state.to_string(),
                pid,
                port,
            });
        }
        self.health = compute_health(&self.services);
    }
}

fn compute_health(services: &[PylonServiceInfo]) -> PylonHealth {
    if services.is_empty() {
        return PylonHealth::Orange;
    }
    let hpprd = services.iter().find(|s| s.name == "hpprd");
    match hpprd {
        Some(s) if s.state == "running" || s.state == "external" => PylonHealth::Green,
        Some(s) if s.state == "starting" => PylonHealth::Orange,
        Some(_) => PylonHealth::Red,
        None => PylonHealth::Orange,
    }
}

const DOT_GREEN: [f32; 4] = [0.267, 0.733, 0.267, 1.0]; // #44bb44
const DOT_ORANGE: [f32; 4] = [0.867, 0.533, 0.0, 1.0]; // #dd8800
const DOT_RED: [f32; 4] = [0.867, 0.2, 0.2, 1.0]; // #dd3333
const MENU_WIDTH: f64 = 240.0;

impl App {
    /// Set the pylon dot color based on current health.
    pub(super) fn update_pylon_dot(&self, cx: &mut Cx) {
        let color = match self.pylon_status.health {
            PylonHealth::Booting => DOT_ORANGE,
            PylonHealth::Green => DOT_GREEN,
            PylonHealth::Orange => DOT_ORANGE,
            PylonHealth::Red => DOT_RED,
        };
        let dot = self.ui.view(cx, ids!(pylon_dot));
        if let Some(mut v) = dot.borrow_mut() {
            v.draw_bg.draw_vars.set_uniform(cx, live_id!(color), &color);
        }
        cx.redraw_all();
    }

    /// Show the pylon dropdown menu anchored near the dot.
    pub(super) fn show_pylon_menu(&mut self, cx: &mut Cx) {
        self.pylon_menu_open = true;

        // Build menu content text.
        let header = format!("Pylon: {}", self.pylon_status.mode);
        self.ui
            .label(cx, ids!(pylon_menu_header))
            .set_text(cx, &header);

        // Build services text.
        let mut svc_lines = String::new();
        for svc in &self.pylon_status.services {
            let dot_char = match svc.state.as_str() {
                "running" | "external" => "\u{25CF}", // ●
                "starting" => "\u{25D4}",             // ◔
                _ => "\u{25CB}",                      // ○
            };
            svc_lines.push_str(&format!("  {} {}", dot_char, svc.name));
            if let Some(port) = svc.port {
                svc_lines.push_str(&format!(" :{}", port));
            }
            svc_lines.push('\n');
        }
        if self.pylon_status.services.is_empty() {
            svc_lines.push_str("  (no services)\n");
        }

        // Mounts
        if !self.pylon_status.mounts.is_empty() {
            svc_lines.push_str("\nMounts:\n");
            for m in &self.pylon_status.mounts {
                svc_lines.push_str(&format!("  {} ({})\n", m.mountpoint, m.fstype));
            }
        }

        let svc_text = svc_lines.trim_end().to_string();
        self.ui
            .label(cx, ids!(pylon_menu_services))
            .set_text(cx, &svc_text);

        // Determine which action buttons to show.
        let hpprd_running = self.pylon_status.services.iter().any(|s| {
            s.name == "hpprd" && (s.state == "running" || s.state == "external")
        });
        let nfs_running = self.pylon_status.services.iter().any(|s| {
            s.name == "hppr-nfs" && s.state == "running"
        });
        let has_mount = !self.pylon_status.mounts.is_empty();
        let is_local = self.pylon_status.mode == "local";

        // hpprd start/stop only in local mode
        self.ui
            .button(cx, ids!(pylon_hpprd_start_btn))
            .set_visible(cx, is_local && !hpprd_running);
        self.ui
            .button(cx, ids!(pylon_hpprd_stop_btn))
            .set_visible(cx, is_local && hpprd_running);

        self.ui
            .button(cx, ids!(pylon_nfs_start_btn))
            .set_visible(cx, !nfs_running);
        self.ui
            .button(cx, ids!(pylon_nfs_stop_btn))
            .set_visible(cx, nfs_running);

        self.ui
            .button(cx, ids!(pylon_mount_btn))
            .set_visible(cx, !has_mount);
        self.ui
            .button(cx, ids!(pylon_unmount_btn))
            .set_visible(cx, has_mount);

        // Position the menu below the pylon_dot, right-aligned.
        let dot_rect = self.ui.view(cx, ids!(pylon_dot)).area().rect(cx);
        let content_rect = self.ui.view(cx, ids!(content_area)).area().rect(cx);

        let content_right = content_rect.pos.x + content_rect.size.x;
        let mut menu_x = (content_right - MENU_WIDTH - 4.0).max(content_rect.pos.x);
        let menu_y = if self.menu_at_bottom {
            dot_rect.pos.y - 10.0
        } else {
            dot_rect.pos.y + dot_rect.size.y + 4.0
        };

        // Fallback if dot_rect is available, prefer aligning to the dot's right edge.
        if dot_rect.size.x > 0.0 {
            let right_aligned = dot_rect.pos.x + dot_rect.size.x - MENU_WIDTH;
            if right_aligned >= content_rect.pos.x {
                menu_x = right_aligned;
            }
        }

        let menu = self.ui.view(cx, ids!(pylon_menu));
        menu.set_visible(cx, true);
        if let Some(mut v) = menu.borrow_mut() {
            v.walk.abs_pos = Some(dvec2(menu_x, menu_y));
        }
        cx.redraw_all();
    }

    pub(super) fn hide_pylon_menu(&mut self, cx: &mut Cx) {
        self.pylon_menu_open = false;
        self.ui.view(cx, ids!(pylon_menu)).set_visible(cx, false);
        cx.redraw_all();
    }

    /// Refresh pylon status via the command client. Call from frame loop.
    pub(super) fn refresh_pylon_status(&mut self, cx: &mut Cx) {
        if let Some(ref mut client) = self.pylon_command_client {
            if let Ok(data) = client.command("status", None, None) {
                self.pylon_status = PylonStatus::from_json(&data);
                self.update_pylon_dot(cx);
            }
        }
    }

    /// Run a pylon command (start/stop/mount/unmount). Updates status after.
    pub(super) fn pylon_command(
        &mut self,
        cx: &mut Cx,
        cmd: &str,
        service: Option<&str>,
        args: Option<&serde_json::Map<String, serde_json::Value>>,
    ) {
        if let Some(ref mut client) = self.pylon_command_client {
            let _ = client.command(cmd, service, args);
        }
        self.refresh_pylon_status(cx);
    }
}
