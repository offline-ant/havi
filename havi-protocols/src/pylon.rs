/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pylon service manager client.
//!
//! Connects to a running pylon instance via TCP JSON lines protocol.
//! No dependency on the pylon crate — communicates purely over TCP.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Port file name within config dir.
const PORT_FILENAME: &str = "pylon.port";

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

impl PylonClient {
    /// Try to connect to a running pylon by reading the port file.
    /// Returns None if pylon is not running or unreachable.
    pub fn try_connect() -> Option<Self> {
        let port = read_port_file()?;
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

    /// Get the hpprd port from pylon status. Returns None if hpprd is not running.
    pub fn hpprd_port(&mut self) -> Option<u16> {
        let services = self.status().ok()?;
        services.iter()
            .find(|s| s.name == "hpprd" && s.state == "running")
            .and_then(|s| s.port)
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

    /// Start hpprd via pylon and wait for its port. Retries status up to 10 times.
    pub fn start_hpprd(&mut self) -> Option<u16> {
        // Already running?
        if let Some(port) = self.hpprd_port() {
            return Some(port);
        }
        // Request start
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
/// Returns a connected PylonClient.
pub fn ensure_pylon() -> Option<PylonClient> {
    // Try existing pylon first
    if let Some(client) = PylonClient::try_connect() {
        return Some(client);
    }

    // Start pylon as a background subprocess
    use std::process::{Command, Stdio};
    let mut child = Command::new("pylon")
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

/// Read pylon port from config file.
fn read_port_file() -> Option<u16> {
    let path = pylon_port_path();
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Path to the pylon port file.
fn pylon_port_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(dir).join("pylon").join(PORT_FILENAME)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config").join("pylon").join(PORT_FILENAME)
    } else {
        PathBuf::from("/tmp/hppr").join(PORT_FILENAME)
    }
}
