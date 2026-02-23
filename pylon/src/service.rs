//! Service process management.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
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

    fn reset_runtime_state(&mut self) {
        self.state = State::Stopped;
        self.pid = None;
        self.port = None;
        self.child = None;
        self.stdin = None;
        self.stdout_tx = None;
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.clear();
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

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                self.reset_runtime_state();
                return Err(format_spawn_error(&self.name, program, args, &e));
            },
        };
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

        self.reset_runtime_state();

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

fn format_spawn_error(
    service: &str,
    program: &str,
    args: &[String],
    error: &std::io::Error,
) -> String {
    let args_json = serde_json::to_string(args).unwrap_or_else(|_| "[]".to_string());
    let resolved_program = describe_program_resolution(program);
    let cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "<unavailable>".to_string());
    let path_snapshot = truncate_for_log(
        &std::env::var("PATH").unwrap_or_else(|_| "<unset>".to_string()),
        512,
    );
    let hint = spawn_hint(program, error.kind());

    format!(
        "service start failed: service={} program={} resolved_program={} args={} cwd={} PATH={} error={} hint={}",
        service, program, resolved_program, args_json, cwd, path_snapshot, error, hint,
    )
}

fn spawn_hint(program: &str, error_kind: std::io::ErrorKind) -> String {
    if error_kind == std::io::ErrorKind::NotFound {
        return format!(
            "ensure '{}' is installed or reachable via PATH; in this repo use hack/{} or pass args.program",
            program, program
        );
    }
    "check executable permissions and runtime environment".to_string()
}

fn describe_program_resolution(program: &str) -> String {
    let path = Path::new(program);
    if path.components().count() > 1 {
        if path.exists() {
            return path.display().to_string();
        }
        return format!("{} (missing)", path.display());
    }

    match resolve_program_on_path(program) {
        Some(path) => path.display().to_string(),
        None => format!("{} (not found on PATH)", program),
    }
}

fn resolve_program_on_path(program: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

fn truncate_for_log(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in value.chars().take(max_chars) {
        out.push(ch);
    }
    if value.chars().count() > max_chars {
        out.push('…');
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn start_spawn_error_resets_service_state() {
        let mut svc = ManagedService::new("hppr-fuse");
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let err = svc
            .start(
                "/definitely/missing/hppr-fuse",
                &[],
                &HashMap::new(),
                "hppr-fuse mounted on ",
                event_tx,
            )
            .await
            .expect_err("spawn should fail");

        assert_eq!(svc.state, State::Stopped);
        assert!(svc.pid.is_none());
        assert!(svc.port.is_none());
        assert!(svc.child.is_none());
        assert!(svc.stdin.is_none());
        assert!(svc.stdout_tx.is_none());
        assert!(err.contains("service start failed:"));
        assert!(err.contains("service=hppr-fuse"));
        assert!(err.contains("program=/definitely/missing/hppr-fuse"));
        assert!(err.contains("resolved_program=/definitely/missing/hppr-fuse (missing)"));
        assert!(err.contains("PATH="));
        assert!(err.contains("hint=ensure '/definitely/missing/hppr-fuse' is installed"));
    }

    #[test]
    fn describe_program_resolution_reports_missing_path_programs() {
        let description = describe_program_resolution("/definitely/missing/hppr-fuse");
        assert_eq!(description, "/definitely/missing/hppr-fuse (missing)");
    }
}

mod libc {
    unsafe extern "C" {
        pub fn kill(pid: i32, sig: i32) -> i32;
    }
    pub const SIGTERM: i32 = 15;
}
