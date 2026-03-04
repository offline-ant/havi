/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HAVI single-instance IPC.
//!
//! Unix targets use a fixed socket at `<config_dir>/havi.sock`.
//! Non-Unix targets use localhost TCP with a fallback `<config_dir>/havi.port`.

use std::io::{BufRead, BufReader, Write};
use std::sync::OnceLock;
use std::sync::mpsc;

use serde::{Deserialize, Serialize};

/// IPC command from second instance to first.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum IpcCommand {
    #[serde(rename = "open")]
    Open { url: String },
}

/// IPC response from first instance.
#[derive(Debug, Serialize, Deserialize)]
pub struct IpcResponse {
    pub ok: bool,
}

#[cfg(unix)]
fn ipc_socket_path() -> std::path::PathBuf {
    crate::config::ipc_socket_path()
}

#[cfg(not(unix))]
fn ipc_port_path() -> std::path::PathBuf {
    crate::config::config_dir().join("havi.port")
}

/// Try to connect to a running HAVI instance and send a command.
/// Returns Ok(()) if the command was accepted, Err if no instance is running.
pub fn try_send_open(url: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::net::UnixStream;

        let path = ipc_socket_path();
        let mut stream =
            UnixStream::connect(&path).map_err(|e| format!("no running instance: {}", e))?;

        let cmd = IpcCommand::Open {
            url: url.to_string(),
        };
        let mut line = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
        line.push('\n');
        stream
            .write_all(line.as_bytes())
            .map_err(|e| e.to_string())?;
        stream.flush().map_err(|e| e.to_string())?;

        let mut reader = BufReader::new(&stream);
        let mut resp_line = String::new();
        reader
            .read_line(&mut resp_line)
            .map_err(|e| e.to_string())?;
        let resp: IpcResponse =
            serde_json::from_str(&resp_line).map_err(|e| format!("bad response: {}", e))?;

        if resp.ok {
            Ok(())
        } else {
            Err("rejected".to_string())
        }
    }

    #[cfg(not(unix))]
    {
        use std::net::TcpStream;

        let path = ipc_port_path();
        let port_str =
            std::fs::read_to_string(&path).map_err(|e| format!("no running instance: {}", e))?;
        let port: u16 = port_str
            .trim()
            .parse()
            .map_err(|e| format!("bad port file: {}", e))?;

        let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| {
            let _ = std::fs::remove_file(&path);
            format!("no running instance: {}", e)
        })?;

        let cmd = IpcCommand::Open {
            url: url.to_string(),
        };
        let mut line = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
        line.push('\n');
        stream
            .write_all(line.as_bytes())
            .map_err(|e| e.to_string())?;
        stream.flush().map_err(|e| e.to_string())?;

        let mut reader = BufReader::new(&stream);
        let mut resp_line = String::new();
        reader
            .read_line(&mut resp_line)
            .map_err(|e| e.to_string())?;
        let resp: IpcResponse =
            serde_json::from_str(&resp_line).map_err(|e| format!("bad response: {}", e))?;

        if resp.ok {
            Ok(())
        } else {
            Err("rejected".to_string())
        }
    }
}

/// Start the IPC listener. Returns a receiver for incoming commands.
/// The listener runs on a background thread.
pub fn start_ipc_listener() -> Result<mpsc::Receiver<IpcCommand>, String> {
    #[cfg(unix)]
    {
        use std::os::unix::net::{UnixListener, UnixStream};

        let socket_path = ipc_socket_path();

        if socket_path.exists() {
            if UnixStream::connect(&socket_path).is_ok() {
                return Err("another instance is listening".to_string());
            }
            let _ = std::fs::remove_file(&socket_path);
        }

        if let Some(parent) = socket_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let listener = UnixListener::bind(&socket_path).map_err(|e| {
            format!(
                "failed to bind unix socket '{}': {}",
                socket_path.display(),
                e
            )
        })?;

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let tx = tx.clone();
                std::thread::spawn(move || {
                    handle_ipc_client(stream, tx);
                });
            }
            let _ = std::fs::remove_file(ipc_socket_path());
        });

        Ok(rx)
    }

    #[cfg(not(unix))]
    {
        use std::net::{TcpListener, TcpStream};

        let port_path = ipc_port_path();

        if port_path.exists() {
            if let Ok(port_str) = std::fs::read_to_string(&port_path) {
                if let Ok(port) = port_str.trim().parse::<u16>() {
                    if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                        return Err("another instance is listening".to_string());
                    }
                }
            }
            let _ = std::fs::remove_file(&port_path);
        }

        if let Some(parent) = port_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let listener =
            TcpListener::bind("127.0.0.1:0").map_err(|e| format!("failed to bind: {}", e))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("local_addr: {}", e))?
            .port();

        std::fs::write(&port_path, port.to_string())
            .map_err(|e| format!("write port file: {}", e))?;

        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let tx = tx.clone();
                std::thread::spawn(move || {
                    handle_ipc_client(stream, tx);
                });
            }
            let _ = std::fs::remove_file(ipc_port_path());
        });

        Ok(rx)
    }
}

/// Remove the IPC endpoint. Call on clean shutdown.
pub fn cleanup_ipc_socket() {
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(ipc_socket_path());
    }

    #[cfg(not(unix))]
    {
        let _ = std::fs::remove_file(ipc_port_path());
    }
}

#[cfg(unix)]
fn handle_ipc_client(stream: std::os::unix::net::UnixStream, tx: mpsc::Sender<IpcCommand>) {
    let reader = BufReader::new(&stream);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.is_empty() {
            continue;
        }
        let Ok(cmd) = serde_json::from_str::<IpcCommand>(&line) else {
            continue;
        };
        let ok = tx.send(cmd).is_ok();
        let resp = IpcResponse { ok };
        let mut writer = &stream;
        let mut resp_line = serde_json::to_string(&resp).unwrap_or_default();
        resp_line.push('\n');
        let _ = writer.write_all(resp_line.as_bytes());
        let _ = writer.flush();
        makepad_platform_signal();
    }
}

#[cfg(not(unix))]
fn handle_ipc_client(stream: std::net::TcpStream, tx: mpsc::Sender<IpcCommand>) {
    let reader = BufReader::new(&stream);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.is_empty() {
            continue;
        }
        let Ok(cmd) = serde_json::from_str::<IpcCommand>(&line) else {
            continue;
        };
        let ok = tx.send(cmd).is_ok();
        let resp = IpcResponse { ok };
        let mut writer = &stream;
        let mut resp_line = serde_json::to_string(&resp).unwrap_or_default();
        resp_line.push('\n');
        let _ = writer.write_all(resp_line.as_bytes());
        let _ = writer.flush();
        makepad_platform_signal();
    }
}

static SIGNAL_CALLBACK: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Set the callback to wake the UI event loop. Called once by the shell at startup.
pub fn set_signal_callback(f: impl Fn() + Send + Sync + 'static) {
    let _ = SIGNAL_CALLBACK.set(Box::new(f));
}

/// Wake the Makepad UI event loop from a background thread.
fn makepad_platform_signal() {
    if let Some(f) = SIGNAL_CALLBACK.get() {
        f();
    }
}
