/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR watch primitives for live-reload support.
//!
//! Two layers:
//! - `WatchPool` — shared connection pool keyed by `//group/app/`
//! - `WatchHandle` — per-tab watch state with mode and event filtering

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use crate::client::get_admin_credentials;
use crate::url::HAVIAddress;

/// Per-tab watch mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WatchMode {
    #[default]
    Off,
    Notify,
    Auto,
    Dev,
}

impl WatchMode {
    /// Cycle to next mode.
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Notify,
            Self::Notify => Self::Auto,
            Self::Auto => Self::Dev,
            Self::Dev => Self::Off,
        }
    }

    /// Button label text.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "W:Off",
            Self::Notify => "W:Note",
            Self::Auto => "W:Auto",
            Self::Dev => "W:Dev",
        }
    }
}

/// Result of polling a watch handle for events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchAction {
    None,
    ChangeDetected,
    Reload,
}

// ---------------------------------------------------------------------------
// WatchConn — shared per //group/app/
// ---------------------------------------------------------------------------

/// Shared watch connection for a `//group/app/` prefix.
///
/// Owns a tokio task that streams WATCH events and distributes them to
/// subscribers via `std::sync::mpsc` channels.
pub struct WatchConn {
    #[allow(dead_code)]
    group_app: String,
    task: tokio::task::JoinHandle<()>,
    subscribers: Arc<Mutex<Vec<std::sync::mpsc::Sender<String>>>>,
}

impl WatchConn {
    /// Spawn a new watch connection for the given `//group/app/` prefix.
    fn spawn(group_app: &str, wake_fn: fn()) -> Self {
        let subscribers: Arc<Mutex<Vec<std::sync::mpsc::Sender<String>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let subs = subscribers.clone();
        let urc = format!("{}/", group_app); // trailing slash for prefix watch
        let task = tokio::spawn(async move {
            let (ring1_name, token) = get_admin_credentials();
            let signer = hppr_client::Signer::ring1_adhoc(&ring1_name, &token);
            let endpoint = hppr_client::repo_endpoint();
            let addr: std::net::SocketAddr = match endpoint.parse() {
                Ok(a) => a,
                Err(e) => {
                    log::error!("watch: invalid repo endpoint '{}': {}", endpoint, e);
                    return;
                }
            };
            let (events_tx, mut events_rx) = tokio::sync::mpsc::channel::<String>(64);
            tokio::spawn(hppr_client::watch_stream(addr, signer, urc, events_tx));
            while let Some(line) = events_rx.recv().await {
                let mut subs = subs.lock().unwrap();
                subs.retain(|tx| tx.send(line.clone()).is_ok());
                drop(subs);
                wake_fn();
            }
        });
        Self {
            group_app: group_app.to_string(),
            task,
            subscribers,
        }
    }

    /// Add a new subscriber. Returns the receiver end.
    pub fn subscribe(&self) -> std::sync::mpsc::Receiver<String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.subscribers.lock().unwrap().push(tx);
        rx
    }
}

impl Drop for WatchConn {
    fn drop(&mut self) {
        self.task.abort();
    }
}

// ---------------------------------------------------------------------------
// WatchPool — one per process, lives on App
// ---------------------------------------------------------------------------

/// Pool of shared watch connections keyed by `//group/app/`.
pub struct WatchPool {
    conns: HashMap<String, Weak<WatchConn>>,
    wake_fn: fn(),
}

impl WatchPool {
    /// Create a new pool with the given wake function.
    pub fn new(wake_fn: fn()) -> Self {
        Self {
            conns: HashMap::new(),
            wake_fn,
        }
    }

    /// Get or create a shared connection for the given `//group/app/` prefix.
    pub fn get_or_create(&mut self, group_app: &str) -> Arc<WatchConn> {
        // Try to upgrade existing weak ref
        if let Some(weak) = self.conns.get(group_app) {
            if let Some(arc) = weak.upgrade() {
                return arc;
            }
        }
        // Clean dead entries opportunistically
        self.conns.retain(|_, w| w.strong_count() > 0);
        // Spawn new connection
        let conn = Arc::new(WatchConn::spawn(group_app, self.wake_fn));
        self.conns.insert(group_app.to_string(), Arc::downgrade(&conn));
        conn
    }
}

// ---------------------------------------------------------------------------
// WatchHandle — per-tab, lives on TabInfo
// ---------------------------------------------------------------------------

/// Per-tab watch state. Holds mode, connection, and event receiver.
pub struct WatchHandle {
    mode: WatchMode,
    conn: Option<Arc<WatchConn>>,
    rx: Option<std::sync::mpsc::Receiver<String>>,
    /// The `//group/app/` prefix this handle is watching.
    active_group_app: Option<String>,
    /// Full coordinate of the tab's current page (e.g. `//g/a/path`).
    active_urc: Option<String>,
    /// Whether a change has been detected (for Notify mode indicator).
    pub change_detected: bool,
}

impl Default for WatchHandle {
    fn default() -> Self {
        Self {
            mode: WatchMode::Off,
            conn: None,
            rx: None,
            active_group_app: None,
            active_urc: None,
            change_detected: false,
        }
    }
}

impl WatchHandle {
    /// Current mode.
    pub fn mode(&self) -> WatchMode {
        self.mode
    }

    /// Set mode and reset change_detected.
    pub fn set_mode(&mut self, mode: WatchMode) {
        self.mode = mode;
        self.change_detected = false;
    }

    /// Clear the change_detected flag (on navigation).
    pub fn clear_change_detected(&mut self) {
        self.change_detected = false;
    }

    /// Reconcile watch state with the tab's current URL.
    ///
    /// Acquires or releases connections from the pool as needed based on
    /// current mode and URL.
    pub fn reconcile(&mut self, url: &str, pool: &mut WatchPool) {
        if self.mode == WatchMode::Off {
            self.stop();
            return;
        }

        // Parse URL to extract group/app
        let (group_app, urc) = match extract_group_app_and_urc(url) {
            Some(v) => v,
            None => {
                self.stop();
                return;
            }
        };

        // If already watching the right group/app, just update the URC
        if self.active_group_app.as_deref() == Some(&group_app) {
            self.active_urc = Some(urc);
            return;
        }

        // Need a different connection — release old, acquire new
        self.conn = None;
        self.rx = None;
        let conn = pool.get_or_create(&group_app);
        let rx = conn.subscribe();
        self.conn = Some(conn);
        self.rx = Some(rx);
        self.active_group_app = Some(group_app);
        self.active_urc = Some(urc);
        self.change_detected = false;
    }

    /// Poll for watch events. Returns the highest-priority action.
    pub fn poll(&mut self) -> WatchAction {
        let rx = match &self.rx {
            Some(rx) => rx,
            None => return WatchAction::None,
        };

        let mut action = WatchAction::None;
        while let Ok(line) = rx.try_recv() {
            let matches = match self.mode {
                WatchMode::Off => false,
                WatchMode::Dev => true, // any event under //group/app/
                WatchMode::Auto | WatchMode::Notify => {
                    // Match if the event line contains the tab's exact coordinate
                    match &self.active_urc {
                        Some(urc) => line.contains(urc),
                        None => false,
                    }
                }
            };
            if matches {
                let new_action = match self.mode {
                    WatchMode::Notify => WatchAction::ChangeDetected,
                    WatchMode::Auto | WatchMode::Dev => WatchAction::Reload,
                    WatchMode::Off => WatchAction::None,
                };
                // Escalate: Reload > ChangeDetected > None
                if (new_action as u8) > (action as u8) {
                    action = new_action;
                }
            }
        }

        if action == WatchAction::ChangeDetected {
            self.change_detected = true;
        }

        action
    }

    /// Release connection and clear state.
    pub fn stop(&mut self) {
        self.conn = None;
        self.rx = None;
        self.active_group_app = None;
        self.active_urc = None;
        self.change_detected = false;
    }
}

/// Extract `//group/app` prefix and full URC from a URL string.
fn extract_group_app_and_urc(url: &str) -> Option<(String, String)> {
    let addr = HAVIAddress::parse(url).ok()?;
    let parts = addr.parts();
    if parts.group.is_empty() || parts.app.is_empty() {
        return None;
    }
    let group_app = format!("//{}/{}", parts.group, parts.app);
    let urc = addr.urc_string();
    Some((group_app, urc))
}
