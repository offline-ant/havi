//! Service process management.

use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

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
        }
    }

    /// Start the service as a child process.
    ///
    /// `program` and `args` define the command. `env` sets extra environment
    /// variables. `port_pattern` is a prefix to look for in stdout to extract
    /// the bound port (e.g. "HPPRD_BIND=").
    ///
    /// Returns a channel that receives the bound port once detected.
    pub async fn start(
        &mut self,
        program: &str,
        args: &[String],
        env: &HashMap<String, String>,
        port_pattern: &str,
        event_tx: mpsc::UnboundedSender<ServiceEvent>,
    ) -> Result<(), String> {
        if self.state != State::Stopped {
            return Err(format!("{} is already {}", self.name, state_str(self.state)));
        }

        self.state = State::Starting;

        let mut cmd = Command::new(program);
        cmd.args(args)
            .envs(env.iter())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| format!("spawn {}: {}", program, e))?;
        let pid = child.id();
        self.pid = pid;

        let stdout = child.stdout.take();
        self.child = Some(child);

        // Spawn stdout reader to extract port
        let name = self.name.clone();
        let pattern = port_pattern.to_string();
        tokio::spawn(async move {
            let mut port_found = None;
            if let Some(stdout) = stdout {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::info!("[{}] {}", name, line);
                    if port_found.is_none() {
                        if let Some(rest) = line.strip_prefix(&pattern) {
                            // Extract port from "host:port" or just "port"
                            let port_str = rest.rsplit(':').next().unwrap_or(rest);
                            // Strip trailing / or whitespace
                            let port_str = port_str.trim_end_matches('/').trim();
                            if let Ok(p) = port_str.parse::<u16>() {
                                port_found = Some(p);
                                let _ = event_tx.send(ServiceEvent {
                                    name: name.clone(),
                                    state: State::Running,
                                    pid,
                                    port: Some(p),
                                    exit_code: None,
                                });
                            }
                        }
                    }
                }
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

    /// Stop the service. Sends SIGTERM, waits up to `timeout_secs`, then SIGKILL.
    pub async fn stop(&mut self) -> Result<Option<i32>, String> {
        if self.state == State::Stopped {
            return Ok(None);
        }
        self.state = State::Stopping;

        if let Some(ref mut child) = self.child {
            // Send SIGTERM via kill
            let pid = child.id();
            if let Some(pid) = pid {
                unsafe { libc::kill(pid as i32, libc::SIGTERM); }
            }

            // Wait with timeout
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                child.wait(),
            ).await;

            let exit_code = match result {
                Ok(Ok(status)) => status.code(),
                Ok(Err(_)) => None,
                Err(_) => {
                    // Timeout — force kill
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    None
                }
            };

            self.child = None;
            self.state = State::Stopped;
            self.pid = None;
            self.port = None;
            Ok(exit_code)
        } else {
            self.state = State::Stopped;
            self.pid = None;
            self.port = None;
            Ok(None)
        }
    }

    /// Status as JSON value.
    pub fn status_json(&self) -> serde_json::Value {
        serde_json::json!({
            "state": self.state,
            "pid": self.pid,
            "port": self.port,
        })
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
    extern "C" {
        pub fn kill(pid: i32, sig: i32) -> i32;
    }
    pub const SIGTERM: i32 = 15;
}
