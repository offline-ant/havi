/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pylon service manager client.
//!
//! Connects to a running pylon instance via TCP JSON lines protocol.
//! No dependency on the pylon crate — communicates purely over TCP.

use anyhow::{Context, anyhow};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// PID file name within repo directory.
const PID_FILENAME: &str = "pylon.pid";

#[cfg(target_os = "windows")]
const PYLON_BIN_NAMES: &[&str] = &["pylon.exe"];
#[cfg(not(target_os = "windows"))]
const PYLON_BIN_NAMES: &[&str] = &["pylon"];

#[derive(Debug, Clone)]
struct PylonLaunch {
    program: std::path::PathBuf,
    use_havi_subcommand: bool,
}

/// Connection to a running pylon instance.
pub struct PylonClient {
    stream: BufReader<TcpStream>,
    next_id: AtomicU64,
    pub port: u16,
}

/// Status of a single pylon service.
#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub name: String,
    pub state: String,
    pub pid: Option<u32>,
    pub port: Option<u16>,
}

/// Event broadcast from pylon.
#[derive(Debug, Clone)]
pub struct PylonEvent {
    pub event: String,
    pub service: Option<String>,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub listener: Option<String>,
    pub present: Option<bool>,
    pub public_ip: Option<String>,
    pub public_via: Option<String>,
    pub source: Option<String>,
}

impl PylonClient {
    /// Try to connect to a running pylon by reading the port file.
    /// Returns None if pylon is not running or unreachable.
    pub fn try_connect(repo_path: &std::path::Path) -> Option<Self> {
        let (_, port) = read_pid_file(repo_path)?;
        Self::connect(port).ok()
    }

    fn try_connect_with_error(repo_path: &std::path::Path) -> anyhow::Result<Option<Self>> {
        let (pid, port) = match read_pid_file(repo_path) {
            Some(v) => v,
            None => return Ok(None),
        };
        match Self::connect(port) {
            Ok(client) => Ok(Some(client)),
            Err(_) if !is_pid_alive(pid) => {
                // Stale pid file — process is gone. Clean up and let caller spawn a new one.
                let _ = std::fs::remove_file(repo_path.join(PID_FILENAME));
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Connect to pylon at the given port.
    pub fn connect(port: u16) -> anyhow::Result<Self> {
        let stream = TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", port).parse().unwrap(),
            Duration::from_secs(2),
        )
        .with_context(|| format!("pylon control connect to 127.0.0.1:{} failed", port))?;
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(2))).ok();
        Ok(Self {
            stream: BufReader::new(stream),
            next_id: AtomicU64::new(1),
            port,
        })
    }

    /// Send a command and receive the response.
    fn request(
        &mut self,
        cmd: &str,
        service: Option<&str>,
        args: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut req = serde_json::json!({"id": id, "cmd": cmd});
        if let Some(svc) = service {
            req["service"] = serde_json::json!(svc);
        }
        if let Some(cmd_args) = args {
            req["args"] = serde_json::Value::Object(cmd_args.clone());
        }

        let mut line = serde_json::to_string(&req).unwrap();
        line.push('\n');
        self.stream
            .get_mut()
            .write_all(line.as_bytes())
            .map_err(|e| format!("pylon write: {}", e))?;

        // Read lines until we get our response (skip event broadcasts)
        loop {
            let mut resp_line = String::new();
            self.stream
                .read_line(&mut resp_line)
                .map_err(|e| format!("pylon read: {}", e))?;
            if resp_line.is_empty() {
                return Err("pylon: connection closed".to_string());
            }
            let resp: serde_json::Value =
                serde_json::from_str(&resp_line).map_err(|e| format!("pylon parse: {}", e))?;

            // Skip event broadcasts (they have "event" field, not "id")
            if resp.get("event").is_some() {
                continue;
            }
            if resp.get("id").and_then(|v| v.as_u64()) == Some(id) {
                if resp.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                    return Ok(resp.get("data").cloned().unwrap_or(serde_json::Value::Null));
                } else {
                    let err = resp
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown error");
                    return Err(format!("pylon: {}", err));
                }
            }
        }
    }

    /// Query status of all services.
    pub fn status(&mut self) -> Result<Vec<ServiceStatus>, String> {
        let data = self.request("status", None, None)?;
        let obj = data.as_object().ok_or("pylon: status not an object")?;
        let mut services = Vec::new();
        for (name, val) in obj {
            services.push(ServiceStatus {
                name: name.clone(),
                state: val
                    .get("state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                pid: val.get("pid").and_then(|v| v.as_u64()).map(|n| n as u32),
                port: val.get("port").and_then(|v| v.as_u64()).map(|n| n as u16),
            });
        }
        services.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(services)
    }

    /// Get the hpprd port from pylon status.
    ///
    /// In local mode, returns the port from hpprd's running state.
    /// In remote mode, parses the port from the external address.
    pub fn hpprd_port(&mut self) -> Option<u16> {
        let data = self.request("status", None, None).ok()?;
        let hpprd = data.get("hpprd")?;
        let state = hpprd.get("state")?.as_str()?;
        match state {
            "running" => hpprd.get("port")?.as_u64().map(|n| n as u16),
            "external" => {
                // Remote mode: parse port from addr "host:port"
                let addr = hpprd.get("addr")?.as_str()?;
                addr.rsplit(':').next()?.parse().ok()
            },
            _ => None,
        }
    }

    /// Start a service.
    pub fn start_service(&mut self, name: &str) -> Result<(), String> {
        self.request("start", Some(name), None)?;
        Ok(())
    }

    /// Stop a service.
    pub fn stop_service(&mut self, name: &str) -> Result<(), String> {
        self.request("stop", Some(name), None)?;
        Ok(())
    }

    /// Send an arbitrary pylon command with optional service and args.
    pub fn command(
        &mut self,
        cmd: &str,
        service: Option<&str>,
        args: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<serde_json::Value, String> {
        self.request(cmd, service, args)
    }

    /// Subscribe to pylon events. Consumes the client and spawns a reader thread.
    /// The TCP connection stays open (preventing pylon idle shutdown).
    /// Returns a receiver for events.
    pub fn subscribe(self) -> std::sync::mpsc::Receiver<PylonEvent> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut stream = self.stream;
            let mut line = String::new();
            loop {
                line.clear();
                match stream.read_line(&mut line) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        // Parse event JSON
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                            if let Some(event) = val.get("event").and_then(|v| v.as_str()) {
                                let ev = PylonEvent {
                                    event: event.to_string(),
                                    service: val
                                        .get("service")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                    pid: val.get("pid").and_then(|v| v.as_u64()).map(|n| n as u32),
                                    port: val
                                        .get("port")
                                        .and_then(|v| v.as_u64())
                                        .map(|n| n as u16),
                                    listener: val
                                        .get("listener")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                    present: val.get("present").and_then(|v| v.as_bool()),
                                    public_ip: val
                                        .get("public_ip")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                    public_via: match val.get("public_via") {
                                        Some(serde_json::Value::String(s)) => Some(s.to_string()),
                                        _ => None,
                                    },
                                    source: val
                                        .get("source")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                };
                                if tx.send(ev).is_err() {
                                    break; // Receiver dropped
                                }
                            }
                        }
                    },
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        continue; // Read timeout, keep going
                    },
                    Err(_) => break, // Connection error
                }
            }
        });
        rx
    }

    /// Ensure hpprd is available and return its port.
    ///
    /// In local mode, starts hpprd via pylon and polls for the port.
    /// In remote mode, returns the external hpprd port immediately.
    pub fn start_hpprd(&mut self) -> anyhow::Result<u16> {
        // Already running or external?
        if let Some(port) = self.hpprd_port() {
            return Ok(port);
        }

        // Request start (this can fail if pylon rejects the request).
        let start_error = self.start_service("hpprd").err();

        // Poll for port (hpprd needs time to bind and report status).
        let mut last_state: Option<String> = None;
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(300));
            if let Some(port) = self.hpprd_port() {
                return Ok(port);
            }

            if let Ok(status) = self.request("status", None, None) {
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
        }

        let mut message = String::from(
            "hpprd did not become reachable after pylon startup request and 10 status polls (~3s).",
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
        } else {
            message.push_str(" Pylon status did not return an hpprd entry during polling.");
        }
        Err(anyhow!(message))
    }
}

/// Ensure a pylon instance is running. Starts one as a subprocess if needed.
///
/// `home` selects remote mode: pylon connects satellites to an external hpprd
/// at that address instead of spawning its own.
///
/// Returns a connected PylonClient.
pub fn ensure_pylon(
    repo_path: &std::path::Path,
    home: Option<&str>,
) -> anyhow::Result<PylonClient> {
    ensure_pylon_with_self_exec_process_fallback(repo_path, home, false)
}

/// Ensure a pylon instance is running.
///
/// Launcher lookup order:
/// 1) ./pylon.exe on Windows, ./pylon on non-Windows
/// 2) pylon executable in PATH (pylon.exe on Windows, pylon otherwise)
/// 3) current executable as `havi pylon ...` when Self-Exec Process Runtime fallback is enabled
pub fn ensure_pylon_with_self_exec_process_fallback(
    repo_path: &std::path::Path,
    home: Option<&str>,
    self_exec_process_fallback: bool,
) -> anyhow::Result<PylonClient> {
    // Try existing pylon first.
    match PylonClient::try_connect_with_error(repo_path) {
        Ok(Some(client)) => {
            eprintln!("[havi] pylon: connected to existing instance on port {}", client.port);
            return Ok(client);
        },
        Ok(None) => {
            eprintln!("[havi] pylon: no existing instance found at {}", repo_path.display());
        },
        Err(e) => {
            return Err(e).context(format!(
                "found pylon.pid in '{}' but failed to connect to the recorded control port",
                repo_path.display()
            ));
        },
    }

    let launch = resolve_pylon_launch(self_exec_process_fallback).map_err(|e| {
        eprintln!("[havi] pylon: {:#}", e);
        e
    })?;
    eprintln!("[havi] pylon: spawning {} (havi_subcommand={})",
        launch.program.display(), launch.use_havi_subcommand);
    spawn_pylon_subprocess(&launch, repo_path, home)
}

fn resolve_pylon_launch(self_exec_process_fallback: bool) -> anyhow::Result<PylonLaunch> {
    if let Some(program) = find_pylon_in_current_dir() {
        return Ok(PylonLaunch {
            program,
            use_havi_subcommand: false,
        });
    }

    if let Some(program) = find_pylon_in_path() {
        return Ok(PylonLaunch {
            program,
            use_havi_subcommand: false,
        });
    }

    if self_exec_process_fallback {
        let program = std::env::current_exe().context(
            "cannot resolve current executable path for Self-Exec Process Runtime pylon fallback",
        )?;
        return Ok(PylonLaunch {
            program,
            use_havi_subcommand: true,
        });
    }

    Err(anyhow!(
        "pylon executable not found in ./ or PATH (checked ./pylon.exe on Windows, ./pylon otherwise, then PATH)."
    ))
}

fn find_pylon_in_current_dir() -> Option<std::path::PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    for name in PYLON_BIN_NAMES {
        let candidate = cwd.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn find_pylon_in_path() -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in PYLON_BIN_NAMES {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn spawn_pylon_subprocess(
    launch: &PylonLaunch,
    repo_path: &std::path::Path,
    home: Option<&str>,
) -> anyhow::Result<PylonClient> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(&launch.program);
    if launch.use_havi_subcommand {
        cmd.arg("pylon");
    }
    cmd.arg("--path").arg(repo_path);
    if let Some(addr) = home {
        cmd.arg("--home").arg(addr);
    }

    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "failed to spawn '{}' as pylon subprocess",
                launch.program.display()
            )
        })?;

    let stdout = child
        .stdout
        .take()
        .context("spawned pylon process has no stdout pipe; cannot read PYLON_BIND announcement")?;
    let mut stderr = child
        .stderr
        .take()
        .context("spawned pylon process has no stderr pipe; cannot capture startup diagnostics")?;

    // Read startup stdout for PYLON_BIND= line with a timeout.
    // Pylon should print PYLON_BIND= within seconds; 15s is generous.
    // Use a background thread + channel because pipe reads have no timeout API.
    let (line_tx, line_rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("pylon-stdout-reader".to_string())
        .spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines() {
                if line_tx.send(line).is_err() {
                    break; // receiver dropped
                }
            }
        })
        .context("failed to spawn pylon stdout reader thread")?;

    let mut observed_stdout = Vec::new();
    let startup_deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let remaining = startup_deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        if observed_stdout.len() >= 20 {
            break;
        }
        let line = match line_rx.recv_timeout(remaining) {
            Ok(Ok(l)) => l,
            Ok(Err(e)) => return Err(e).context("failed while reading pylon startup stdout"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        observed_stdout.push(line.clone());
        eprintln!("[havi] pylon stdout: {}", line);
        if let Some(addr) = line.strip_prefix("PYLON_BIND=") {
            let port: u16 = addr
                .rsplit(':')
                .next()
                .ok_or_else(|| anyhow!("invalid PYLON_BIND value '{}' (missing ':<port>')", addr))?
                .parse()
                .with_context(|| {
                    format!("invalid PYLON_BIND value '{}' (port parse failed)", addr)
                })?;

            // Detach child — pylon runs as a daemon.
            std::mem::forget(child);
            std::thread::sleep(Duration::from_millis(100));
            return PylonClient::connect(port).with_context(|| {
                format!(
                    "pylon reported PYLON_BIND=127.0.0.1:{}, but connection failed immediately",
                    port
                )
            });
        }
    }

    let exit_status = child
        .try_wait()
        .context("failed to inspect spawned pylon process status")?;

    let mut stderr_text = String::new();
    if exit_status.is_some() {
        let _ = stderr.read_to_string(&mut stderr_text);
    }

    let stdout_preview = if observed_stdout.is_empty() {
        "(no stdout lines)".to_string()
    } else {
        observed_stdout.join(" | ")
    };

    let mut message = format!(
        "spawned pylon process did not announce PYLON_BIND within 15s / 20 stdout lines. observed stdout: {}.",
        stdout_preview
    );

    if let Some(status) = exit_status {
        message.push_str(&format!(" process exited early with status {}.", status));
    } else {
        message.push_str(" process is still running but did not emit a parseable PYLON_BIND line.");
    }

    if !stderr_text.trim().is_empty() {
        message.push_str(" stderr: ");
        message.push_str(stderr_text.trim());
        message.push('.');
    }

    Err(anyhow!(message))
}

/// Check whether a process with the given PID is still running.
fn is_pid_alive(pid: u32) -> bool {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        std::path::Path::new(&format!("/proc/{}", pid)).exists()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(true)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = pid;
        true // conservative: let connect error propagate normally
    }
}

/// Read pylon PID file from repo directory. Returns (pid, port).
pub fn read_pid_file(repo_path: &std::path::Path) -> Option<(u32, u16)> {
    let content = std::fs::read_to_string(repo_path.join(PID_FILENAME)).ok()?;
    let mut parts = content.trim().split(' ');
    let pid: u32 = parts.next()?.parse().ok()?;
    let port: u16 = parts.next()?.parse().ok()?;
    Some((pid, port))
}
