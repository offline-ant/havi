/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pylon service manager client.
//!
//! Connects to a running pylon instance via TCP JSON lines protocol.
//! No dependency on the pylon crate — communicates purely over TCP.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// PID file name within repo directory.
const PID_FILENAME: &str = "pylon.pid";

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
}

impl PylonClient {
    /// Try to connect to a running pylon by reading the port file.
    /// Returns None if pylon is not running or unreachable.
    pub fn try_connect(repo_path: &std::path::Path) -> Option<Self> {
        let (_, port) = read_pid_file(repo_path)?;
        Self::connect(port).ok()
    }

    /// Connect to pylon at the given port.
    pub fn connect(port: u16) -> Result<Self, String> {
        let stream = TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", port).parse().unwrap(),
            Duration::from_secs(2),
        ).map_err(|e| format!("pylon connect: {}", e))?;
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(2))).ok();
        Ok(Self {
            stream: BufReader::new(stream),
            next_id: AtomicU64::new(1),
            port,
        })
    }

    /// Send a command and receive the response.
    fn request(&mut self, cmd: &str, service: Option<&str>) -> Result<serde_json::Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut req = serde_json::json!({"id": id, "cmd": cmd});
        if let Some(svc) = service {
            req["service"] = serde_json::json!(svc);
        }

        let mut line = serde_json::to_string(&req).unwrap();
        line.push('\n');
        self.stream.get_mut().write_all(line.as_bytes())
            .map_err(|e| format!("pylon write: {}", e))?;

        // Read lines until we get our response (skip event broadcasts)
        loop {
            let mut resp_line = String::new();
            self.stream.read_line(&mut resp_line)
                .map_err(|e| format!("pylon read: {}", e))?;
            if resp_line.is_empty() {
                return Err("pylon: connection closed".to_string());
            }
            let resp: serde_json::Value = serde_json::from_str(&resp_line)
                .map_err(|e| format!("pylon parse: {}", e))?;

            // Skip event broadcasts (they have "event" field, not "id")
            if resp.get("event").is_some() {
                continue;
            }
            if resp.get("id").and_then(|v| v.as_u64()) == Some(id) {
                if resp.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                    return Ok(resp.get("data").cloned().unwrap_or(serde_json::Value::Null));
                } else {
                    let err = resp.get("error").and_then(|v| v.as_str())
                        .unwrap_or("unknown error");
                    return Err(format!("pylon: {}", err));
                }
            }
        }
    }

    /// Query status of all services.
    pub fn status(&mut self) -> Result<Vec<ServiceStatus>, String> {
        let data = self.request("status", None)?;
        let obj = data.as_object().ok_or("pylon: status not an object")?;
        let mut services = Vec::new();
        for (name, val) in obj {
            services.push(ServiceStatus {
                name: name.clone(),
                state: val.get("state").and_then(|v| v.as_str())
                    .unwrap_or("unknown").to_string(),
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
        let data = self.request("status", None).ok()?;
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
        self.request("start", Some(name))?;
        Ok(())
    }

    /// Stop a service.
    pub fn stop_service(&mut self, name: &str) -> Result<(), String> {
        self.request("stop", Some(name))?;
        Ok(())
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
                                    service: val.get("service").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                    pid: val.get("pid").and_then(|v| v.as_u64()).map(|n| n as u32),
                                    port: val.get("port").and_then(|v| v.as_u64()).map(|n| n as u16),
                                };
                                if tx.send(ev).is_err() {
                                    break; // Receiver dropped
                                }
                            }
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {
                        continue; // Read timeout, keep going
                    }
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
    pub fn start_hpprd(&mut self) -> Option<u16> {
        // Already running or external?
        if let Some(port) = self.hpprd_port() {
            return Some(port);
        }
        // Request start (will fail in remote mode, which is fine —
        // hpprd_port() already returned the external port above)
        let _ = self.start_service("hpprd");
        // Poll for port (hpprd needs time to bind)
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(300));
            if let Some(port) = self.hpprd_port() {
                return Some(port);
            }
        }
        None
    }
}

/// Ensure a pylon instance is running. Starts one as a subprocess if needed.
///
/// `home` selects remote mode: pylon connects satellites to an external hpprd
/// at that address instead of spawning its own.
///
/// Returns a connected PylonClient.
pub fn ensure_pylon(repo_path: &std::path::Path, home: Option<&str>) -> Option<PylonClient> {
    // Try existing pylon first
    if let Some(client) = PylonClient::try_connect(repo_path) {
        return Some(client);
    }

    // Start pylon as a background subprocess
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe().ok()?;
    let mut cmd = Command::new(&exe);
    cmd.arg("pylon")
        .arg("--path")
        .arg(repo_path);
    if let Some(addr) = home {
        cmd.arg("--home").arg(addr);
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // Read stdout for PYLON_BIND= line
    use std::io::BufRead;
    let stdout = child.stdout.take()?;
    let reader = std::io::BufReader::new(stdout);
    for line in reader.lines().take(10) {
        let line = line.ok()?;
        if let Some(addr) = line.strip_prefix("PYLON_BIND=") {
            let port: u16 = addr.rsplit(':').next()?.parse().ok()?;
            // Detach child — pylon runs as a daemon
            std::mem::forget(child);
            std::thread::sleep(Duration::from_millis(100));
            return PylonClient::connect(port).ok();
        }
    }
    None
}

/// Read pylon PID file from repo directory. Returns (pid, port).
pub fn read_pid_file(repo_path: &std::path::Path) -> Option<(u32, u16)> {
    let content = std::fs::read_to_string(repo_path.join(PID_FILENAME)).ok()?;
    let mut parts = content.trim().split(' ');
    let pid: u32 = parts.next()?.parse().ok()?;
    let port: u16 = parts.next()?.parse().ok()?;
    Some((pid, port))
}
