//! Service process management.

use std::collections::{BTreeSet, HashMap};
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, broadcast, mpsc};

/// Service runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Stopped,
    Starting,
    Running,
    Stopping,
}

/// A managed service instance.
pub struct ManagedService {
    pub name: String,
    pub state: State,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    child: Option<Child>,
    stdin: Option<Arc<Mutex<ChildStdin>>>,
    stdout_tx: Option<broadcast::Sender<String>>,
    listeners: Arc<StdMutex<BTreeSet<String>>>,
}

/// Notification when a service changes state.
#[derive(Debug)]
pub struct ServiceEvent {
    pub name: String,
    pub state: State,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub exit_code: Option<i32>,
}

impl ManagedService {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            state: State::Stopped,
            pid: None,
            port: None,
            child: None,
            stdin: None,
            stdout_tx: None,
            listeners: Arc::new(StdMutex::new(BTreeSet::new())),
        }
    }

    /// Start the service as a child process.
    ///
    /// `program` and `args` define the command. `env` sets extra environment
    /// variables. `port_pattern` is a prefix to look for in stdout to extract
    /// the bound port (e.g. "HPPRD_LISTEN=").
    pub async fn start(
        &mut self,
        program: &str,
        args: &[String],
        env: &HashMap<String, String>,
        port_pattern: &str,
        event_tx: mpsc::UnboundedSender<ServiceEvent>,
    ) -> Result<(), String> {
        if self.state != State::Stopped {
            return Err(format!(
                "{} is already {}",
                self.name,
                state_str(self.state)
            ));
        }

        self.state = State::Starting;
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.clear();
        }

        let mut cmd = Command::new(program);
        cmd.args(args)
            .envs(env.iter())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("spawn {}: {}", program, e))?;
        let pid = child.id();
        self.pid = pid;

        let stdin = child.stdin.take().map(|stdin| Arc::new(Mutex::new(stdin)));
        let stdout = child.stdout.take();
        let (stdout_tx, _) = broadcast::channel::<String>(256);

        self.stdin = stdin;
        self.stdout_tx = Some(stdout_tx.clone());
        self.child = Some(child);

        // Spawn stdout reader to extract port and forward hpprd listener events.
        let name = self.name.clone();
        let pattern = port_pattern.to_string();
        let listeners = Arc::clone(&self.listeners);
        tokio::spawn(async move {
            let mut port_found = None;
            let mut ready_sent = false;
            if let Some(stdout) = stdout {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::info!("[{}] {}", name, line);
                    let _ = stdout_tx.send(line.clone());

                    for listener_id in parse_listener_add(&line) {
                        if let Ok(mut set) = listeners.lock() {
                            set.insert(listener_id);
                        }
                    }
                    if let Some(listener_id) = parse_listener_remove(&line) {
                        if let Ok(mut set) = listeners.lock() {
                            set.remove(&listener_id);
                        }
                    }

                    if port_found.is_none() && !ready_sent {
                        if let Some(rest) = line.strip_prefix(&pattern) {
                            // Extract port from pattern value.
                            // For HPPRD_LISTEN=tcp+host:port,quib+..., find first tcp+ entry.
                            // For simpler patterns like "hppr-nfs listening on host:port", parse directly.
                            // For hppr-fuse, the value is a path — port stays None.
                            let port_str = rest
                                .split(',')
                                .find_map(|entry| entry.trim().strip_prefix("tcp+"))
                                .unwrap_or(rest);
                            let port_str = port_str.rsplit(':').next().unwrap_or(port_str);
                            let port_str = port_str.trim_end_matches('/').trim();
                            let port = port_str.parse::<u16>().ok();
                            port_found = port;
                            ready_sent = true;
                            let _ = event_tx.send(ServiceEvent {
                                name: name.clone(),
                                state: State::Running,
                                pid,
                                port,
                                exit_code: None,
                            });
                        }
                    }
                }
            }

            if let Ok(mut set) = listeners.lock() {
                set.clear();
            }

            // stdout closed — process likely exited
            let _ = event_tx.send(ServiceEvent {
                name: name.clone(),
                state: State::Stopped,
                pid,
                port: port_found,
                exit_code: None,
            });
        });

        Ok(())
    }

    /// Stop the service. Sends SIGTERM, waits up to 5s, then SIGKILL.
    pub async fn stop(&mut self) -> Result<Option<i32>, String> {
        if self.state == State::Stopped {
            return Ok(None);
        }
        self.state = State::Stopping;

        let exit_code = if let Some(ref mut child) = self.child {
            // Send SIGTERM via kill
            let pid = child.id();
            if let Some(pid) = pid {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
            }

            // Wait with timeout
            let result =
                tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await;

            match result {
                Ok(Ok(status)) => status.code(),
                Ok(Err(_)) => None,
                Err(_) => {
                    // Timeout — force kill
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    None
                },
            }
        } else {
            None
        };

        self.child = None;
        self.stdin = None;
        self.stdout_tx = None;
        self.state = State::Stopped;
        self.pid = None;
        self.port = None;
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.clear();
        }

        Ok(exit_code)
    }

    /// Status as JSON value.
    pub fn status_json(&self) -> serde_json::Value {
        serde_json::json!({
            "state": self.state,
            "pid": self.pid,
            "port": self.port,
        })
    }

    pub fn control_handles(
        &self,
    ) -> Result<(Arc<Mutex<ChildStdin>>, broadcast::Receiver<String>), String> {
        if self.state != State::Running {
            return Err(format!("{} is {}", self.name, state_str(self.state)));
        }
        let stdin = self
            .stdin
            .as_ref()
            .cloned()
            .ok_or_else(|| format!("{} stdin is not available", self.name))?;
        let stdout_rx = self
            .stdout_tx
            .as_ref()
            .ok_or_else(|| format!("{} stdout is not available", self.name))?
            .subscribe();
        Ok((stdin, stdout_rx))
    }

    pub fn listener_snapshot(&self) -> Vec<String> {
        self.listeners
            .lock()
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }
}

/// Parse `HPPRD_LISTEN=` lines into listener IDs.
///
/// Startup format: `HPPRD_LISTEN=tcp+host:port,quib+host:port,...`
/// Dynamic add: `HPPRD_LISTEN=tcp+host:port` (single entry)
///
/// Converts `scheme+addr` to `scheme:addr` for internal listener IDs.
fn parse_listener_add(line: &str) -> Vec<String> {
    let rest = match line.strip_prefix("HPPRD_LISTEN=") {
        Some(r) => r.trim(),
        None => return Vec::new(),
    };
    if rest.is_empty() {
        return Vec::new();
    }
    rest.split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return None;
            }
            // Convert scheme+addr to scheme:addr
            if let Some(pos) = entry.find('+') {
                let scheme = &entry[..pos];
                let addr = &entry[pos + 1..];
                if scheme == "unix" {
                    Some(format!("unix:{}", normalize_unix_path(addr)))
                } else {
                    Some(format!("{}:{}", scheme, addr))
                }
            } else {
                Some(entry.to_string())
            }
        })
        .collect()
}

fn parse_listener_remove(line: &str) -> Option<String> {
    line.strip_prefix("HPPRD_UNLISTEN=")
        .and_then(nonempty)
        .map(|id| id.to_string())
}

fn nonempty(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn normalize_unix_path(path: &str) -> String {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        p.display().to_string()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(p).display().to_string())
            .unwrap_or_else(|_| p.display().to_string())
    }
}

fn state_str(s: State) -> &'static str {
    match s {
        State::Stopped => "stopped",
        State::Starting => "starting",
        State::Running => "running",
        State::Stopping => "stopping",
    }
}

mod libc {
    unsafe extern "C" {
        pub fn kill(pid: i32, sig: i32) -> i32;
    }
    pub const SIGTERM: i32 = 15;
}
