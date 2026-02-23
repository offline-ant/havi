//! pylon CLI binary.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().skip(1).collect();

    // Extract --bind and --path from anywhere in args
    let mut port = pylon::DEFAULT_PORT;
    let mut repo_path = PathBuf::from("./repo");
    let mut positional = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--bind" => {
                if let Some(v) = args.get(i + 1) {
                    if let Some(pos) = v.rfind(':') {
                        port = v[pos + 1..].parse().unwrap_or(pylon::DEFAULT_PORT);
                    } else {
                        port = v.parse().unwrap_or(pylon::DEFAULT_PORT);
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            },
            "--path" => {
                if let Some(v) = args.get(i + 1) {
                    repo_path = PathBuf::from(v);
                    i += 2;
                } else {
                    i += 1;
                }
            },
            _ => {
                positional.push(args[i].clone());
                i += 1;
            },
        }
    }

    if positional.is_empty() {
        run_daemon(port, repo_path);
        return;
    }

    match positional[0].as_str() {
        "--help" | "-h" => print_usage(),

        // Global commands
        "status" => send_command("status", None, &HashMap::new(), &repo_path),
        "mounts" => send_command("mounts", None, &HashMap::new(), &repo_path),
        "shutdown" => send_command("shutdown", None, &HashMap::new(), &repo_path),

        // Service commands: pylon <service> <action> [args]
        "hpprd" | "lokid" | "unlokid" => {
            let service = &positional[0];
            let action = positional.get(1).map(|s| s.as_str()).unwrap_or("start");
            let extra = parse_kv_args(&positional[2..]);
            match action {
                "start" => send_command("start", Some(service), &extra, &repo_path),
                "stop" => send_command("stop", Some(service), &HashMap::new(), &repo_path),
                "listen" if service == "hpprd" => send_command("listen", None, &extra, &repo_path),
                "unlisten" if service == "hpprd" => {
                    send_command("unlisten", None, &extra, &repo_path)
                },
                other => {
                    eprintln!("unknown action for {}: {}", service, other);
                    std::process::exit(1);
                },
            }
        },

        // NFS commands: pylon nfs <action> [mountpoint] [--args]
        "nfs" => {
            let action = positional.get(1).map(|s| s.as_str()).unwrap_or_else(|| {
                eprintln!("usage: pylon nfs <start|stop|mount|unmount>");
                std::process::exit(1);
            });
            match action {
                "start" => {
                    let extra = parse_kv_args(&positional[2..]);
                    send_command("start", Some("hppr-fs"), &extra, &repo_path);
                },
                "stop" => {
                    send_command("stop", Some("hppr-fs"), &HashMap::new(), &repo_path);
                },
                "mount" => {
                    let mut extra = parse_kv_args(&positional[2..]);
                    if let Some(pos) = positional[2..].iter().find(|a| !a.starts_with('-')) {
                        extra
                            .entry("mountpoint".to_string())
                            .or_insert_with(|| pos.clone());
                    }
                    send_command("mount", None, &extra, &repo_path);
                },
                "unmount" => {
                    let mut extra = HashMap::new();
                    if let Some(pos) = positional[2..].iter().find(|a| !a.starts_with('-')) {
                        extra.insert("mountpoint".to_string(), pos.clone());
                    }
                    send_command("unmount", None, &extra, &repo_path);
                },
                other => {
                    eprintln!("unknown nfs action: {}", other);
                    std::process::exit(1);
                },
            }
        },

        other => {
            eprintln!("unknown command: {}", other);
            print_usage();
            std::process::exit(1);
        },
    }
}

fn run_daemon(port: u16, repo_path: PathBuf) {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    if let Err(e) = rt.block_on(pylon::run(port, repo_path)) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn send_command(
    cmd: &str,
    service: Option<&str>,
    args: &HashMap<String, String>,
    repo_path: &std::path::Path,
) {
    let port = pylon::read_pid_file(repo_path)
        .map(|(_, p)| p)
        .unwrap_or(pylon::DEFAULT_PORT);
    let addr = format!("127.0.0.1:{}", port);

    let mut stream = match TcpStream::connect(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot connect to pylon at {}: {}", addr, e);
            std::process::exit(1);
        },
    };

    let req_id = 1u64;
    let mut req = serde_json::json!({"id": req_id, "cmd": cmd});
    if let Some(svc) = service {
        req["service"] = serde_json::json!(svc);
    }
    if !args.is_empty() {
        let obj: serde_json::Map<String, serde_json::Value> = args
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::json!(v)))
            .collect();
        req["args"] = serde_json::Value::Object(obj);
    }

    let mut line = serde_json::to_string(&req).unwrap();
    line.push('\n');
    stream.write_all(line.as_bytes()).unwrap();
    stream.flush().unwrap();

    let mut reader = BufReader::new(&stream);
    loop {
        let mut resp_line = String::new();
        if reader.read_line(&mut resp_line).unwrap_or(0) == 0 {
            eprintln!("error: connection closed");
            std::process::exit(1);
        }

        let resp: serde_json::Value = match serde_json::from_str(&resp_line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Ignore unsolicited events and unrelated responses.
        if resp.get("event").is_some() {
            continue;
        }
        if resp.get("id").and_then(|v| v.as_u64()) != Some(req_id) {
            continue;
        }

        if resp.get("ok") == Some(&serde_json::json!(true)) {
            if let Some(data) = resp.get("data") {
                println!("{}", serde_json::to_string_pretty(data).unwrap());
            } else {
                println!("ok");
            }
        } else {
            let err = resp
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            eprintln!("error: {}", err);
            std::process::exit(1);
        }
        break;
    }
}

fn parse_kv_args(args: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if let Some(key) = arg.strip_prefix("--") {
            if let Some(val) = args.get(i + 1) {
                map.insert(key.to_string(), val.to_string());
                i += 2;
            } else {
                map.insert(key.to_string(), "true".to_string());
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    map
}

fn print_usage() {
    eprintln!("Usage: pylon [COMMAND]");
    eprintln!();
    eprintln!("Daemon mode:");
    eprintln!("  pylon                            Start the pylon daemon");
    eprintln!(
        "  pylon --bind [host:]<port>        Control address (default: 127.0.0.1:{})",
        pylon::DEFAULT_PORT
    );
    eprintln!("  pylon --path <path>              Repository path (default: ./repo)");
    eprintln!();
    eprintln!("Global commands:");
    eprintln!("  pylon status                     Show all service status + mounts");
    eprintln!("  pylon mounts                     List active NFS mounts");
    eprintln!("  pylon shutdown                   Stop all services and exit");
    eprintln!();
    eprintln!("Service commands:");
    eprintln!("  pylon hpprd start [--k v]        Start hpprd");
    eprintln!("  pylon hpprd stop                 Stop hpprd");
    eprintln!("  pylon hpprd listen --bind <spec> Add hpprd listener at runtime");
    eprintln!("  pylon hpprd unlisten --bind <id|spec> Remove hpprd listener");
    eprintln!("  pylon lokid start [--k v]        Start lokid");
    eprintln!("  pylon lokid stop                 Stop lokid");
    eprintln!("  pylon unlokid start [--k v]      Start unlokid");
    eprintln!("  pylon unlokid stop               Stop unlokid");
    eprintln!();
    eprintln!("NFS commands:");
    eprintln!("  pylon nfs start [--k v]          Start hppr-fs server");
    eprintln!("  pylon nfs stop                   Stop hppr-fs server");
    eprintln!("  pylon nfs mount [path] [--k v]   Start hppr-fs + OS mount");
    eprintln!("  pylon nfs unmount [path]         OS unmount");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  pylon hpprd start --repo_path /data/repo --bind 127.0.0.1:4777");
    eprintln!("  pylon hpprd listen --bind ws+127.0.0.1:4778");
    eprintln!("  pylon hpprd unlisten --bind ws:127.0.0.1:4778");
    eprintln!("  pylon nfs mount /mnt/hppr --root //u/");
    eprintln!("  pylon nfs unmount /mnt/hppr");
}
