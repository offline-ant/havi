//! pylon: HPPR service manager.
//!
//! Manages hpprd and satellite services (lokid, unlokid, hppr-nfs) as child
//! processes. Exposes a TCP JSON lines control protocol.
//!
//! Use as a library (in-process, e.g. from HAVI) or as a standalone binary.

pub mod cli;
pub mod control;
pub mod mount;
pub mod protocol;
pub mod service;
pub mod services;

mod libc {
    // TODO: Windows: LockFileEx
    unsafe extern "C" {
        pub fn flock(fd: i32, operation: i32) -> i32;
    }
    pub const LOCK_EX: i32 = 2;
    pub const LOCK_NB: i32 = 4;
    pub const EWOULDBLOCK: i32 = 11;
}

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;

use service::{ManagedService, ServiceEvent, State};

/// Default pylon control port (start of scan range).
pub const DEFAULT_PORT: u16 = 4850;

/// Last pylon control port to try (inclusive).
pub const DEFAULT_PORT_END: u16 = 4900;

/// PID file location (relative to config dir).
pub const PID_FILENAME: &str = "pylon.pid";

/// Owns all managed services.
pub struct Pylon {
    services: HashMap<String, ManagedService>,
    event_tx: mpsc::UnboundedSender<ServiceEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<ServiceEvent>>,
    /// hpprd address, set when hpprd starts. Used by other services.
    hpprd_addr: Option<String>,
    shutdown_tx: Option<tokio::sync::watch::Sender<bool>>,
    /// Repository path for hpprd.
    pub repo_path: PathBuf,
}

impl Pylon {
    /// Create a new pylon instance.
    pub fn new(repo_path: PathBuf) -> Self {
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
            repo_path,
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
        let mut args = args.clone();
        // Inject repo_path for hpprd
        if name == "hpprd" && !args.contains_key("repo_path") {
            args.insert(
                "repo_path".to_string(),
                serde_json::json!(self.repo_path.to_string_lossy()),
            );
        }
        // Inject hpprd address for dependent services
        if name != "hpprd" {
            if let Some(ref addr) = self.hpprd_addr {
                if !args.contains_key("home") {
                    match name {
                        "hppr-nfs" | "unlokid" => {
                            args.insert("home".to_string(), serde_json::json!(addr));
                        },
                        _ => {},
                    }
                }
            }
        }

        let (program, cmd_args, env, pattern) = services::resolve(name, &args)?;
        let svc = self
            .services
            .get_mut(name)
            .ok_or_else(|| format!("unknown service: {}", name))?;
        svc.start(&program, &cmd_args, &env, &pattern, self.event_tx.clone())
            .await
    }

    /// Stop a service by name.
    pub async fn stop_service(&mut self, name: &str) -> Result<Option<i32>, String> {
        let svc = self
            .services
            .get_mut(name)
            .ok_or_else(|| format!("unknown service: {}", name))?;
        svc.stop().await
    }

    /// Get status of all services.
    pub fn status(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (name, svc) in &self.services {
            let mut status = svc.status_json();
            if name == "hpprd" {
                if let Some(obj) = status.as_object_mut() {
                    obj.insert(
                        "listeners".to_string(),
                        serde_json::json!(svc.listener_snapshot()),
                    );
                }
            }
            map.insert(name.clone(), status);
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

    /// Get hpprd service state.
    pub fn hpprd_state(&self) -> State {
        self.services.get("hpprd").map_or(State::Stopped, |s| s.state)
    }

    /// Get the hppr-nfs port if running.
    pub fn hppr_nfs_port(&self) -> Option<u16> {
        self.services.get("hppr-nfs").and_then(|s| s.port)
    }

    /// Is hppr-nfs stopped?
    pub fn hppr_nfs_stopped(&self) -> bool {
        self.services
            .get("hppr-nfs")
            .map_or(true, |s| s.state == State::Stopped)
    }

    /// Get hpprd stdin/stdout control handles.
    pub fn hpprd_control_handles(
        &self,
    ) -> Result<
        (
            std::sync::Arc<tokio::sync::Mutex<tokio::process::ChildStdin>>,
            tokio::sync::broadcast::Receiver<String>,
        ),
        String,
    > {
        let svc = self
            .services
            .get("hpprd")
            .ok_or_else(|| "unknown service: hpprd".to_string())?;
        svc.control_handles()
    }

    /// Shutdown all services.
    pub async fn shutdown(&mut self) {
        // Stop in reverse dependency order: satellites first, then hpprd
        for name in &["hppr-nfs", "unlokid", "lokid", "hpprd"] {
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

/// Bind the pylon control port. If `port` is `Some`, bind exactly that port.
/// If `None`, scan `DEFAULT_PORT..=DEFAULT_PORT_END`, then try port 0 (OS
/// random) as a last resort.
async fn bind_control_port(port: Option<u16>) -> Result<tokio::net::TcpListener, String> {
    if let Some(p) = port {
        return tokio::net::TcpListener::bind(format!("127.0.0.1:{}", p))
            .await
            .map_err(|e| format!("bind 127.0.0.1:{}: {}", p, e));
    }

    for p in DEFAULT_PORT..=DEFAULT_PORT_END {
        if let Ok(listener) = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", p)).await {
            return Ok(listener);
        }
    }

    // All ports in range busy — let OS pick a free port.
    tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("bind 127.0.0.1:0: {}", e))
}

/// Run pylon as a standalone daemon.
///
/// Binds the control TCP port and processes commands until shutdown.
/// If `port` is `None`, scans `DEFAULT_PORT..=DEFAULT_PORT_END` for a free
/// port, falling back to a random OS-assigned port.
pub async fn run(port: Option<u16>, repo_path: PathBuf) -> Result<(), String> {
    // Acquire exclusive flock on PID file — prevents dual instances.
    // The lock is held for the process lifetime via _pid_lock.
    let _pid_lock = acquire_pid_lock(&repo_path)?;

    let listener = bind_control_port(port).await?;
    let actual_port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    write_pid_file(&repo_path, std::process::id(), actual_port);
    println!("PYLON_BIND=127.0.0.1:{}", actual_port);

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let mut pylon_inner = Pylon::new(repo_path.clone());
    pylon_inner.set_shutdown(shutdown_tx);
    let event_rx = pylon_inner.take_event_rx().unwrap();

    // Split event_rx: one for state updates, one for control broadcast
    let (state_tx, mut state_rx) = mpsc::unbounded_channel();
    let (broadcast_tx, broadcast_rx) = mpsc::unbounded_channel();

    let pylon = std::sync::Arc::new(tokio::sync::Mutex::new(pylon_inner));

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
        let pylon_auto = Arc::clone(&pylon);
        tokio::spawn(async move {
            let mut y = pylon_auto.lock().await;
            if let Err(e) = y.start_service("hpprd", &HashMap::new()).await {
                log::warn!("auto-start hpprd failed: {}", e);
            }
        });
    }

    // State update loop
    {
        let pylon_state = std::sync::Arc::clone(&pylon);
        tokio::spawn(async move {
            while let Some(event) = state_rx.recv().await {
                let mut y = pylon_state.lock().await;
                y.handle_event(&event);
            }
        });
    }

    control::run(listener, pylon, broadcast_rx, shutdown_rx).await;

    remove_pid_file(&repo_path);
    Ok(())
}

/// Acquire exclusive flock on the PID file.
///
/// Returns the locked file handle. Keep it alive to maintain the lock —
/// the kernel releases it automatically when the process exits (including
/// SIGKILL).
fn acquire_pid_lock(repo_path: &Path) -> Result<File, String> {
    let _ = std::fs::create_dir_all(repo_path);
    let pid_path = repo_path.join(PID_FILENAME);

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&pid_path)
        .map_err(|e| format!("open PID file {}: {}", pid_path.display(), e))?;

    let fd = file.as_raw_fd();
    let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };

    if result != 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
            // Another pylon holds the lock — read PID for diagnostics
            let content = std::fs::read_to_string(&pid_path).unwrap_or_default();
            let pid = content.split_whitespace().next().unwrap_or("unknown");
            return Err(format!("pylon already running (pid {})", pid));
        }
        return Err(format!("flock PID file: {}", err));
    }

    Ok(file)
}

fn write_pid_file(repo_path: &Path, pid: u32, port: u16) {
    let pid_path = repo_path.join(PID_FILENAME);
    // Truncate and write — file is already locked by acquire_pid_lock.
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&pid_path)
    {
        let _ = writeln!(f, "{} {}", pid, port);
    }
}

fn remove_pid_file(repo_path: &Path) {
    let _ = std::fs::remove_file(repo_path.join(PID_FILENAME));
}

/// Read the pylon PID file. Returns `(pid, port)` if found.
pub fn read_pid_file(repo_path: &Path) -> Option<(u32, u16)> {
    let content = std::fs::read_to_string(repo_path.join(PID_FILENAME)).ok()?;
    let mut parts = content.trim().split(' ');
    let pid: u32 = parts.next()?.parse().ok()?;
    let port: u16 = parts.next()?.parse().ok()?;
    Some((pid, port))
}
