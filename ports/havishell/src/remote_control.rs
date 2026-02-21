use makepad_widgets::Cx;
use makepad_widgets::cx_stdin::HostToStdin;
use makepad_widgets::makepad_micro_serde::DeJson;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use std::os::unix::net::UnixListener;

#[derive(Clone, Debug)]
pub enum RemoteCommand {
    Navigate { url: String },
    Back,
    Forward,
    Reload,
    Tabs,
    Screenshot { path: String },
}

pub enum RemoteAction {
    Input(HostToStdin),
    Command {
        cmd: RemoteCommand,
        respond: mpsc::Sender<String>,
    },
}

impl std::fmt::Debug for RemoteAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input(msg) => f.debug_tuple("Input").field(msg).finish(),
            Self::Command { cmd, .. } => f.debug_struct("Command").field("cmd", cmd).finish(),
        }
    }
}



fn parse_command(json: &str) -> Option<RemoteCommand> {
    // Minimal JSON field extraction for "cmd"
    let trimmed = json.trim();
    if !trimmed.starts_with('{') {
        return None;
    }

    // Find "cmd" field value
    let cmd_key = "\"cmd\"";
    let cmd_pos = trimmed.find(cmd_key)?;
    let after_key = &trimmed[cmd_pos + cmd_key.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();

    // Extract the string value
    if !after_colon.starts_with('"') {
        return None;
    }
    let rest = &after_colon[1..];
    let end_quote = rest.find('"')?;
    let cmd_value = &rest[..end_quote];

    match cmd_value {
        "navigate" => {
            let url = extract_string_field(trimmed, "url")?;
            Some(RemoteCommand::Navigate { url })
        }
        "back" => Some(RemoteCommand::Back),
        "forward" => Some(RemoteCommand::Forward),
        "reload" => Some(RemoteCommand::Reload),
        "tabs" => Some(RemoteCommand::Tabs),
        "screenshot" => {
            let path = extract_string_field(trimmed, "path").unwrap_or_else(|| "screenshot.png".to_string());
            Some(RemoteCommand::Screenshot { path })
        }
        _ => None,
    }
}

fn extract_string_field(json: &str, field: &str) -> Option<String> {
    let key = format!("\"{}\"", field);
    let pos = json.find(&key)?;
    let after_key = &json[pos + key.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    if !after_colon.starts_with('"') {
        return None;
    }
    let rest = &after_colon[1..];
    // Handle escaped quotes
    let mut result = String::new();
    let mut chars = rest.chars();
    loop {
        match chars.next()? {
            '\\' => {
                match chars.next()? {
                    '"' => result.push('"'),
                    '\\' => result.push('\\'),
                    'n' => result.push('\n'),
                    't' => result.push('\t'),
                    c => {
                        result.push('\\');
                        result.push(c);
                    }
                }
            }
            '"' => break,
            c => result.push(c),
        }
    }
    Some(result)
}

fn handle_connection<R: std::io::Read, W: Write>(reader: R, mut writer: W) {
    let buf_reader = BufReader::new(reader);
    for line in buf_reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check if this is a custom command (has "cmd" field)
        if let Some(cmd) = parse_command(trimmed) {
            let (tx, rx) = mpsc::channel();
            Cx::post_action(RemoteAction::Command {
                cmd,
                respond: tx,
            });
            // Wait for response from the app
            match rx.recv_timeout(std::time::Duration::from_secs(10)) {
                Ok(response) => {
                    let _ = writeln!(writer, "{}", response);
                    let _ = writer.flush();
                }
                Err(_) => {
                    let _ = writeln!(writer, r#"{{"error":"timeout"}}"#);
                    let _ = writer.flush();
                }
            }
        } else {
            // Try to deserialize as HostToStdin
            match HostToStdin::deserialize_json(trimmed) {
                Ok(msg) => {
                    Cx::post_action(RemoteAction::Input(msg));
                    let _ = writeln!(writer, r#"{{"ok":true}}"#);
                    let _ = writer.flush();
                }
                Err(e) => {
                    let _ = writeln!(writer, r#"{{"error":"parse error: {}"}}"#, e.msg);
                    let _ = writer.flush();
                }
            }
        }
    }
}

enum ListenerKind {
    Tcp(TcpListener),
    Unix(UnixListener),
}

pub fn start_remote_listener() -> Option<()> {
    let addr = std::env::var("HAVI_REMOTE").ok()?;
    if addr.is_empty() {
        return None;
    }

    let listener = if addr.starts_with('/') || addr.starts_with("./") {
        let _ = std::fs::remove_file(&addr);
        let l = UnixListener::bind(&addr).ok()?;
        eprintln!("HAVI_REMOTE={}", addr);
        ListenerKind::Unix(l)
    } else {
        let port: u16 = addr.parse().ok()?;
        let l = TcpListener::bind(("127.0.0.1", port)).ok()?;
        let local = l.local_addr().ok()?;
        eprintln!("HAVI_REMOTE={}", local.port());
        ListenerKind::Tcp(l)
    };

    std::thread::Builder::new()
        .name("havi-remote-listener".into())
        .spawn(move || match listener {
            ListenerKind::Tcp(l) => {
                for stream in l.incoming() {
                    if let Ok(stream) = stream {
                        let reader = stream.try_clone().unwrap();
                        let writer = stream;
                        std::thread::Builder::new()
                            .name("havi-remote-conn".into())
                            .spawn(move || handle_connection(reader, writer))
                            .ok();
                    }
                }
            }
            ListenerKind::Unix(l) => {
                for stream in l.incoming() {
                    if let Ok(stream) = stream {
                        let reader = stream.try_clone().unwrap();
                        let writer = stream;
                        std::thread::Builder::new()
                            .name("havi-remote-conn".into())
                            .spawn(move || handle_connection(reader, writer))
                            .ok();
                    }
                }
            }
        })
        .ok()?;

    Some(())
}
