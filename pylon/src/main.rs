//! pylon CLI binary.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        // Daemon mode
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
        "status" => send_command("status", None, &HashMap::new()),
        "list" => send_command("list", None, &HashMap::new()),
        "shutdown" => send_command("shutdown", None, &HashMap::new()),
        "start" => {
            let service = args.get(1).map(|s| s.as_str());
            let extra = parse_kv_args(&args[2..]);
            send_command("start", service, &extra);
        }
        "stop" => {
            let service = args.get(1).map(|s| s.as_str());
            send_command("stop", service, &HashMap::new());
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
            eprintln!("cannot connect to yard at {}: {}", addr, e);
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

    // Read one response
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
    eprintln!("Daemon mode (no command):");
    eprintln!("  pylon                Start the yard daemon");
    eprintln!("  pylon --port <port>  Start on custom control port (default: {})", pylon::DEFAULT_PORT);
    eprintln!();
    eprintln!("Client commands:");
    eprintln!("  pylon status                     Show all service status");
    eprintln!("  pylon list                       List available services");
    eprintln!("  pylon start <service> [--k v]    Start a service");
    eprintln!("  pylon stop <service>             Stop a service");
    eprintln!("  pylon shutdown                   Stop all services and exit");
    eprintln!();
    eprintln!("Services: hpprd, lokid, unlokid, hppr-fs");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  pylon start hpprd --repo_path /data/repo --bind 127.0.0.1:4777");
    eprintln!("  pylon start lokid --key '&.xxx.H3' --port 4780");
    eprintln!("  pylon start unlokid --port 8080 --shim true");
    eprintln!("  pylon start hppr-fs --port 2049 --root //u/");
}
