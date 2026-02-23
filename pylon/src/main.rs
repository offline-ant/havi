//! pylon CLI binary.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        run_daemon(pylon::DEFAULT_PORT);
        return;
    }

    match args[0].as_str() {
        "--help" | "-h" => print_usage(),
        "--port" => {
            let port: u16 = args.get(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(pylon::DEFAULT_PORT);
            run_daemon(port);
        }

        // Global commands
        "status" => send_command("status", None, &HashMap::new()),
        "mounts" => send_command("mounts", None, &HashMap::new()),
        "shutdown" => send_command("shutdown", None, &HashMap::new()),

        // Service commands: pylon <service> <action> [args]
        "hpprd" | "lokid" | "unlokid" => {
            let service = &args[0];
            let action = args.get(1).map(|s| s.as_str()).unwrap_or("start");
            let extra = parse_kv_args(&args[2..]);
            match action {
                "start" => send_command("start", Some(service), &extra),
                "stop" => send_command("stop", Some(service), &HashMap::new()),
                other => {
                    eprintln!("unknown action for {}: {}", service, other);
                    std::process::exit(1);
                }
            }
        }

        // NFS commands: pylon nfs <action> [mountpoint] [--args]
        "nfs" => {
            let action = args.get(1).map(|s| s.as_str()).unwrap_or_else(|| {
                eprintln!("usage: pylon nfs <start|stop|mount|unmount>");
                std::process::exit(1);
            });
            match action {
                "start" => {
                    let extra = parse_kv_args(&args[2..]);
                    send_command("start", Some("hppr-fs"), &extra);
                }
                "stop" => {
                    send_command("stop", Some("hppr-fs"), &HashMap::new());
                }
                "mount" => {
                    let mut extra = parse_kv_args(&args[2..]);
                    // First non-flag arg is mountpoint
                    if let Some(pos) = args[2..].iter().find(|a| !a.starts_with('-')) {
                        extra.entry("mountpoint".to_string()).or_insert_with(|| pos.clone());
                    }
                    send_command("mount", None, &extra);
                }
                "unmount" => {
                    let mut extra = HashMap::new();
                    // First non-flag arg is mountpoint
                    if let Some(pos) = args[2..].iter().find(|a| !a.starts_with('-')) {
                        extra.insert("mountpoint".to_string(), pos.clone());
                    }
                    send_command("unmount", None, &extra);
                }
                other => {
                    eprintln!("unknown nfs action: {}", other);
                    std::process::exit(1);
                }
            }
        }

        other => {
            eprintln!("unknown command: {}", other);
            print_usage();
            std::process::exit(1);
        }
    }
}

fn run_daemon(port: u16) {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    if let Err(e) = rt.block_on(pylon::run(port)) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn send_command(cmd: &str, service: Option<&str>, args: &HashMap<String, String>) {
    let port = pylon::read_port_file().unwrap_or(pylon::DEFAULT_PORT);
    let addr = format!("127.0.0.1:{}", port);

    let mut stream = match TcpStream::connect(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot connect to pylon at {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    let mut req = serde_json::json!({"id": 1, "cmd": cmd});
    if let Some(svc) = service {
        req["service"] = serde_json::json!(svc);
    }
    if !args.is_empty() {
        let obj: serde_json::Map<String, serde_json::Value> = args.iter()
            .map(|(k, v)| (k.clone(), serde_json::json!(v)))
            .collect();
        req["args"] = serde_json::Value::Object(obj);
    }

    let mut line = serde_json::to_string(&req).unwrap();
    line.push('\n');
    stream.write_all(line.as_bytes()).unwrap();
    stream.flush().unwrap();

    let mut reader = BufReader::new(&stream);
    let mut resp_line = String::new();
    reader.read_line(&mut resp_line).unwrap();

    let resp: serde_json::Value = serde_json::from_str(&resp_line).unwrap_or_default();
    if resp.get("ok") == Some(&serde_json::json!(true)) {
        if let Some(data) = resp.get("data") {
            println!("{}", serde_json::to_string_pretty(data).unwrap());
        } else {
            println!("ok");
        }
    } else {
        let err = resp.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
        eprintln!("error: {}", err);
        std::process::exit(1);
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
    eprintln!("  pylon --port <port>              Custom control port (default: {})", pylon::DEFAULT_PORT);
    eprintln!();
    eprintln!("Global commands:");
    eprintln!("  pylon status                     Show all service status + mounts");
    eprintln!("  pylon mounts                     List active NFS mounts");
    eprintln!("  pylon shutdown                   Stop all services and exit");
    eprintln!();
    eprintln!("Service commands:");
    eprintln!("  pylon hpprd start [--k v]        Start hpprd");
    eprintln!("  pylon hpprd stop                 Stop hpprd");
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
    eprintln!("  pylon nfs mount /mnt/hppr --root //u/");
    eprintln!("  pylon nfs unmount /mnt/hppr");
}
