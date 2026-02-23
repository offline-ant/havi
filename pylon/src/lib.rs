//! pylon: HPPR service manager.
//!
//! Manages hpprd and satellite services (lokid, unlokid, hppr-fs) as child
//! processes. Exposes a TCP JSON lines control protocol.
//!
//! Use as a library (in-process, e.g. from HAVI) or as a standalone binary.

pub mod control;
pub mod protocol;
pub mod service;
pub mod services;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

use service::{ManagedService, ServiceEvent, State};

/// Default control port.
pub const DEFAULT_PORT: u16 = 4850;

/// Port file location (relative to config dir).
pub const PORT_FILENAME: &str = "pylon.port";

/// The yard: owns all managed services.
pub struct Yard {
    services: HashMap<String, ManagedService>,
    event_tx: mpsc::UnboundedSender<ServiceEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<ServiceEvent>>,
    /// hpprd address, set when hpprd starts. Used by other services.
    hpprd_addr: Option<String>,
    shutdown_tx: Option<tokio::sync::watch::Sender<bool>>,
}

impl Default for Yard {
    fn default() -> Self { Self::new() }
}

impl Yard {
    /// Create a new yard.
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let mut services = HashMap::new();
        for name in services::SERVICES {
            services.insert(name.to_string(), ManagedService::new(name));
        }
        Self {
            services,
            event_tx,
            event_rx: Some(event_rx),
            hpprd_addr: None,
            shutdown_tx: None,
        }
    }

    /// Set shutdown sender (called by runner).
    pub fn set_shutdown(&mut self, tx: tokio::sync::watch::Sender<bool>) {
        self.shutdown_tx = Some(tx);
    }

    /// Take the event receiver (call once).
    pub fn take_event_rx(&mut self) -> Option<mpsc::UnboundedReceiver<ServiceEvent>> {
        self.event_rx.take()
    }

    /// Start a service by name.
    pub async fn start_service(
        &mut self,
        name: &str,
        args: &HashMap<String, serde_json::Value>,
    ) -> Result<(), String> {
        // Inject hpprd address for dependent services
        let mut args = args.clone();
        if name != "hpprd" {
            if let Some(ref addr) = self.hpprd_addr {
                if !args.contains_key("repo") && !args.contains_key("pylon") {
                    match name {
                        "hppr-fs" => { args.insert("repo".to_string(), serde_json::json!(addr)); }
                        "unlokid" => { args.insert("pylon".to_string(), serde_json::json!(addr)); }
                        _ => {}
                    }
                }
            }
        }

        let (program, cmd_args, env, pattern) = services::resolve(name, &args)?;
        let svc = self.services.get_mut(name).ok_or_else(|| format!("unknown service: {}", name))?;
        svc.start(&program, &cmd_args, &env, &pattern, self.event_tx.clone()).await
    }

    /// Stop a service by name.
    pub async fn stop_service(&mut self, name: &str) -> Result<Option<i32>, String> {
        let svc = self.services.get_mut(name).ok_or_else(|| format!("unknown service: {}", name))?;
        svc.stop().await
    }

    /// Get status of all services.
    pub fn status(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (name, svc) in &self.services {
            map.insert(name.clone(), svc.status_json());
        }
        serde_json::Value::Object(map)
    }

    /// Process a service event (update internal state).
    pub fn handle_event(&mut self, event: &ServiceEvent) {
        if let Some(svc) = self.services.get_mut(&event.name) {
            svc.state = event.state;
            if event.state == State::Running {
                svc.pid = event.pid;
                svc.port = event.port;
                if event.name == "hpprd" {
                    if let Some(port) = event.port {
                        self.hpprd_addr = Some(format!("127.0.0.1:{}", port));
                    }
                }
            } else if event.state == State::Stopped {
                svc.pid = None;
                svc.port = None;
                if event.name == "hpprd" {
                    self.hpprd_addr = None;
                }
            }
        }
    }

    /// Shutdown all services.
    pub async fn shutdown(&mut self) {
        // Stop in reverse dependency order: satellites first, then hpprd
        for name in &["hppr-fs", "unlokid", "lokid", "hpprd"] {
            if let Some(svc) = self.services.get_mut(*name) {
                if svc.state != State::Stopped {
                    let _ = svc.stop().await;
                }
            }
        }
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }
    }
}

/// Run the yard as a standalone daemon.
///
/// Binds the control TCP port and processes commands until shutdown.
pub async fn run(port: u16) -> Result<(), String> {
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .map_err(|e| format!("bind 127.0.0.1:{}: {}", port, e))?;

    let actual_port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
    write_port_file(actual_port);
    println!("PYLON_BIND=127.0.0.1:{}", actual_port);

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let mut yard_inner = Yard::new();
    yard_inner.set_shutdown(shutdown_tx);
    let event_rx = yard_inner.take_event_rx().unwrap();

    // Split event_rx: one for state updates, one for control broadcast
    let (state_tx, mut state_rx) = mpsc::unbounded_channel();
    let (broadcast_tx, broadcast_rx) = mpsc::unbounded_channel();

    let yard = std::sync::Arc::new(tokio::sync::Mutex::new(yard_inner));

    // Forward events to both channels
    tokio::spawn(async move {
        let mut event_rx = event_rx;
        while let Some(event) = event_rx.recv().await {
            let _ = state_tx.send(ServiceEvent {
                name: event.name.clone(),
                state: event.state,
                pid: event.pid,
                port: event.port,
                exit_code: event.exit_code,
            });
            let _ = broadcast_tx.send(event);
        }
    });

    // Auto-start hpprd on daemon launch.
    {
        let yard3 = Arc::clone(&yard);
        tokio::spawn(async move {
            let mut y = yard3.lock().await;
            if let Err(e) = y.start_service("hpprd", &HashMap::new()).await {
                log::warn!("auto-start hpprd failed: {}", e);
            }
        });
    }

    // State update loop
    {
        let yard2 = std::sync::Arc::clone(&yard);
        tokio::spawn(async move {
            while let Some(event) = state_rx.recv().await {
                let mut y = yard2.lock().await;
                y.handle_event(&event);
            }
        });
    }

    control::run(listener, yard, broadcast_rx, shutdown_rx).await;

    remove_port_file();
    Ok(())
}

fn config_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        std::path::PathBuf::from(dir).join("pylon")
    } else if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home).join(".config").join("pylon")
    } else {
        std::path::PathBuf::from("/tmp/hppr")
    }
}

fn write_port_file(port: u16) {
    let dir = config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(PORT_FILENAME), port.to_string());
}

fn remove_port_file() {
    let _ = std::fs::remove_file(config_dir().join(PORT_FILENAME));
}

/// Read the yard port from the port file. Returns None if not found.
pub fn read_port_file() -> Option<u16> {
    let path = config_dir().join(PORT_FILENAME);
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}
