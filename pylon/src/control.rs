//! TCP control server. Accepts JSON lines connections.
//!
//! Tracks connected clients. When no clients are connected for
//! `IDLE_TIMEOUT`, triggers automatic shutdown.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, broadcast};

use tokio::sync::mpsc;

use crate::Pylon;
use crate::protocol::{Event, Request, Response};

/// Idle timeout: shut down if no clients connect within this duration.
const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Wait limit for hpprd stdin control command ACK/ERROR.
const HPPRD_CONTROL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Additional wait window to collect batched HPPRD_LISTEN/HPPRD_UNLISTEN lines.
const HPPRD_BATCH_WINDOW: std::time::Duration = std::time::Duration::from_millis(25);

/// Run the control server on the given listener.
pub async fn run(
    listener: TcpListener,
    pylon: Arc<Mutex<Pylon>>,
    mut pylon_events: mpsc::UnboundedReceiver<crate::service::ServiceEvent>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let (event_tx, _) = broadcast::channel::<String>(64);
    let client_count = Arc::new(AtomicUsize::new(0));

    // Forward service events to broadcast channel
    let event_tx2 = event_tx.clone();
    tokio::spawn(async move {
        while let Some(svc_event) = pylon_events.recv().await {
            let event = Event {
                event: match svc_event.state {
                    crate::service::State::Running => "service_started".to_string(),
                    crate::service::State::Stopped => "service_stopped".to_string(),
                    _ => format!(
                        "service_{}",
                        serde_json::to_value(svc_event.state).unwrap_or_default()
                    ),
                },
                service: svc_event.name,
                pid: svc_event.pid,
                port: svc_event.port,
                exit_code: svc_event.exit_code,
            };
            if let Ok(line) = serde_json::to_string(&event) {
                let _ = event_tx2.send(line + "\n");
            }
        }
    });

    // Idle shutdown timer: if no clients for IDLE_TIMEOUT, shut down.
    let idle_pylon = Arc::clone(&pylon);
    let idle_count = Arc::clone(&client_count);
    tokio::spawn(async move {
        // Grace period: don't check immediately on startup.
        tokio::time::sleep(IDLE_TIMEOUT).await;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            if idle_count.load(Ordering::Relaxed) == 0 {
                // No clients — start countdown.
                tokio::time::sleep(IDLE_TIMEOUT).await;
                if idle_count.load(Ordering::Relaxed) == 0 {
                    log::info!("no clients for {}s, shutting down", IDLE_TIMEOUT.as_secs());
                    let mut y = idle_pylon.lock().await;
                    y.shutdown().await;
                    break;
                }
            }
        }
    });

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        log::info!("control client connected: {}", addr);
                        client_count.fetch_add(1, Ordering::Relaxed);
                        let pylon = Arc::clone(&pylon);
                        let event_rx = event_tx.subscribe();
                        let ev_tx = event_tx.clone();
                        let cc = Arc::clone(&client_count);
                        tokio::spawn(async move {
                            handle_client(stream, pylon, event_rx, ev_tx).await;
                            cc.fetch_sub(1, Ordering::Relaxed);
                            log::info!("control client disconnected: {}", addr);
                        });
                    }
                    Err(e) => log::error!("accept error: {}", e),
                }
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
        }
    }
}

async fn handle_client(
    stream: TcpStream,
    pylon: Arc<Mutex<Pylon>>,
    mut event_rx: broadcast::Receiver<String>,
    event_tx: broadcast::Sender<String>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    // Spawn event forwarder
    let (write_tx, mut write_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let write_tx2 = write_tx.clone();

    tokio::spawn(async move {
        while let Ok(line) = event_rx.recv().await {
            if write_tx2.send(line).is_err() {
                break;
            }
        }
    });

    // Writer task
    let write_handle = tokio::spawn(async move {
        while let Some(line) = write_rx.recv().await {
            if writer.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    // Read requests
    while let Ok(Some(line)) = lines.next_line().await {
        if line.is_empty() {
            continue;
        }

        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::err(0, format!("parse error: {}", e));
                let msg = serde_json::to_string(&resp).unwrap_or_default() + "\n";
                let _ = write_tx.send(msg);
                continue;
            },
        };

        let resp = dispatch(&pylon, req, &event_tx).await;
        let msg = serde_json::to_string(&resp).unwrap_or_default() + "\n";
        if write_tx.send(msg).is_err() {
            break;
        }
    }

    drop(write_tx);
    let _ = write_handle.await;
}

/// Mount flow: starts hppr-nfs if needed, polls for port (releasing the lock
/// between polls so the state update loop can process events), then runs the
/// OS mount command.
async fn mount_flow(
    pylon: &Arc<Mutex<Pylon>>,
    args: &std::collections::HashMap<String, serde_json::Value>,
) -> Result<String, String> {
    let mountpoint = args
        .get("mountpoint")
        .and_then(|v| v.as_str())
        .unwrap_or(crate::mount::DEFAULT_MOUNTPOINT)
        .to_string();

    // Start hppr-nfs if stopped
    {
        let mut y = pylon.lock().await;
        if y.hppr_nfs_stopped() {
            y.start_service("hppr-nfs", args).await?;
        }
    }

    // Poll for port, releasing lock between attempts
    let mut port = None;
    for _ in 0..30 {
        {
            let y = pylon.lock().await;
            if let Some(p) = y.hppr_nfs_port() {
                port = Some(p);
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let port = port.ok_or("hppr-nfs did not report a port")?;

    let bind = args
        .get("bind")
        .and_then(|v| v.as_str())
        .unwrap_or("127.0.0.1");

    crate::mount::mount(bind, port, &mountpoint).await?;
    Ok(mountpoint)
}

/// FUSE mount flow: starts hppr-fuse (which mounts directly on startup) and
/// waits for the ready indicator.
async fn fuse_mount_flow(
    pylon: &Arc<Mutex<Pylon>>,
    args: &std::collections::HashMap<String, serde_json::Value>,
) -> Result<String, String> {
    #[cfg(not(target_os = "linux"))]
    return Err("hppr-fuse is Linux-only".to_string());

    #[cfg(target_os = "linux")]
    {
        let mountpoint = args
            .get("mountpoint")
            .and_then(|v| v.as_str())
            .unwrap_or(crate::mount::DEFAULT_MOUNTPOINT)
            .to_string();

        // Create mountpoint directory
        tokio::fs::create_dir_all(&mountpoint)
            .await
            .map_err(|e| format!("create {}: {}", mountpoint, e))?;

        // Inject mountpoint into args for the service
        let mut svc_args = args.clone();
        svc_args
            .entry("mountpoint".to_string())
            .or_insert_with(|| serde_json::json!(&mountpoint));

        // Start hppr-fuse if stopped
        {
            let mut y = pylon.lock().await;
            if y.hppr_fuse_stopped() {
                y.start_service("hppr-fuse", &svc_args).await?;
            } else {
                return Err("hppr-fuse is already running".to_string());
            }
        }

        // Poll for running state
        for _ in 0..50 {
            {
                let y = pylon.lock().await;
                match y.service_state("hppr-fuse") {
                    crate::service::State::Running => return Ok(mountpoint),
                    crate::service::State::Stopped => {
                        return Err("hppr-fuse exited unexpectedly".to_string());
                    },
                    _ => {},
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        Err("hppr-fuse did not become ready".to_string())
    }
}

/// FUSE unmount flow: fusermount3 -u, then stop the service.
async fn fuse_unmount_flow(
    pylon: &Arc<Mutex<Pylon>>,
    mountpoint: &str,
) -> Result<(), String> {
    // fusermount3 -u causes hppr-fuse to exit cleanly
    crate::mount::fuse_unmount(mountpoint).await?;

    // Stop the service (it may already be exiting)
    let mut y = pylon.lock().await;
    if !y.hppr_fuse_stopped() {
        let _ = y.stop_service("hppr-fuse").await;
    }
    Ok(())
}

/// Unified FS mount: FUSE on Linux, NFS on macOS/Windows.
async fn fs_mount_flow(
    pylon: &Arc<Mutex<Pylon>>,
    args: &std::collections::HashMap<String, serde_json::Value>,
) -> Result<String, String> {
    if cfg!(target_os = "linux") {
        fuse_mount_flow(pylon, args).await
    } else {
        mount_flow(pylon, args).await
    }
}

/// Unified FS unmount: FUSE on Linux, NFS on macOS/Windows.
async fn fs_unmount_flow(
    pylon: &Arc<Mutex<Pylon>>,
    mountpoint: &str,
) -> Result<(), String> {
    if cfg!(target_os = "linux") {
        fuse_unmount_flow(pylon, mountpoint).await
    } else {
        crate::mount::unmount(mountpoint).await
    }
}

async fn hpprd_listener_flow(
    pylon: &Arc<Mutex<Pylon>>,
    cmd: &str,
    bind: &str,
) -> Result<Vec<String>, String> {
    let (stdin, mut stdout_rx) = {
        let y = pylon.lock().await;
        y.hpprd_control_handles()?
    };

    let request = serde_json::json!({"cmd": cmd, "bind": bind}).to_string() + "\n";
    {
        let mut stdin = stdin.lock().await;
        stdin
            .write_all(request.as_bytes())
            .await
            .map_err(|e| format!("hpprd stdin write failed: {}", e))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("hpprd stdin flush failed: {}", e))?;
    }

    let expected_prefix = match cmd {
        "listen" => "HPPRD_LISTEN=",
        "unlisten" => "HPPRD_UNLISTEN=",
        _ => return Err(format!("unsupported hpprd listener cmd: {}", cmd)),
    };

    let mut matches = Vec::new();

    let first = tokio::time::timeout(HPPRD_CONTROL_TIMEOUT, async {
        loop {
            match stdout_rx.recv().await {
                Ok(line) => {
                    if let Some(err) = line.strip_prefix("HPPRD_ERROR=") {
                        return Err(err.trim().to_string());
                    }
                    if let Some(id) = line.strip_prefix(expected_prefix) {
                        return Ok(id.trim().to_string());
                    }
                },
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return Err("hpprd stdout closed".to_string());
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    continue;
                },
            }
        }
    })
    .await
    .map_err(|_| "hpprd control timeout".to_string())??;

    matches.push(first);

    loop {
        let next = tokio::time::timeout(HPPRD_BATCH_WINDOW, stdout_rx.recv()).await;
        let Ok(result) = next else {
            break;
        };
        match result {
            Ok(line) => {
                if let Some(err) = line.strip_prefix("HPPRD_ERROR=") {
                    return Err(err.trim().to_string());
                }
                if let Some(id) = line.strip_prefix(expected_prefix) {
                    matches.push(id.trim().to_string());
                }
            },
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                return Err("hpprd stdout closed".to_string());
            },
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
        }
    }

    Ok(matches)
}

async fn dispatch(
    pylon: &Arc<Mutex<Pylon>>,
    req: Request,
    event_tx: &broadcast::Sender<String>,
) -> Response {
    // Emit command event for mutating commands
    match req.cmd.as_str() {
        "start" | "stop" | "mount" | "unmount" | "listen" | "unlisten" | "shutdown" => {
            let mut ev = serde_json::json!({"event": "command", "cmd": req.cmd});
            if let Some(ref svc) = req.service {
                ev["service"] = serde_json::json!(svc);
            }
            let _ = event_tx.send(serde_json::to_string(&ev).unwrap() + "\n");
        },
        _ => {},
    }

    let mut y = pylon.lock().await;
    match req.cmd.as_str() {
        "status" => {
            let mut data = y.status();
            let mode_name = y.mode_name().to_string();
            drop(y);
            let all_mounts = crate::mount::list_all_mounts().await;
            let mounts: Vec<_> = all_mounts
                .iter()
                .map(|(dev, mp, fs)| serde_json::json!({"device": dev, "mountpoint": mp, "fstype": fs}))
                .collect();
            let obj = data.as_object_mut().unwrap();
            obj.insert("mode".to_string(), serde_json::json!(mode_name));
            obj.insert("mounts".to_string(), serde_json::json!(mounts));
            obj.insert(
                "user".to_string(),
                serde_json::json!(std::env::var("USER").unwrap_or_default()),
            );
            Response::ok(req.id, data)
        },
        "list" => Response::ok(
            req.id,
            serde_json::json!({
                "services": crate::services::SERVICES,
            }),
        ),
        "start" => {
            let Some(name) = req.service.as_deref() else {
                return Response::err(req.id, "missing 'service' field");
            };
            match y.start_service(name, &req.args).await {
                Ok(()) => Response::ok_empty(req.id),
                Err(e) => Response::err(req.id, e),
            }
        },
        "stop" => {
            let Some(name) = req.service.as_deref() else {
                return Response::err(req.id, "missing 'service' field");
            };
            match y.stop_service(name).await {
                Ok(_) => Response::ok_empty(req.id),
                Err(e) => Response::err(req.id, e),
            }
        },
        "listen" => {
            if matches!(y.mode, crate::PylonMode::Remote { .. }) {
                return Response::err(req.id, "listen not available in remote mode");
            }
            let Some(bind) = req.args.get("bind").and_then(|v| v.as_str()) else {
                return Response::err(req.id, "missing args.bind");
            };
            drop(y);
            match hpprd_listener_flow(pylon, "listen", bind).await {
                Ok(listeners) => Response::ok(req.id, serde_json::json!({"listeners": listeners})),
                Err(e) => Response::err(req.id, e),
            }
        },
        "unlisten" => {
            if matches!(y.mode, crate::PylonMode::Remote { .. }) {
                return Response::err(req.id, "unlisten not available in remote mode");
            }
            let Some(bind) = req.args.get("bind").and_then(|v| v.as_str()) else {
                return Response::err(req.id, "missing args.bind");
            };
            drop(y);
            match hpprd_listener_flow(pylon, "unlisten", bind).await {
                Ok(listeners) => Response::ok(req.id, serde_json::json!({"listeners": listeners})),
                Err(e) => Response::err(req.id, e),
            }
        },
        "mount" => {
            drop(y);
            match fs_mount_flow(pylon, &req.args).await {
                Ok(mp) => Response::ok(req.id, serde_json::json!({"mountpoint": mp})),
                Err(e) => Response::err(req.id, e),
            }
        },
        "unmount" => {
            let mountpoint = req
                .args
                .get("mountpoint")
                .and_then(|v| v.as_str())
                .unwrap_or(crate::mount::DEFAULT_MOUNTPOINT);
            drop(y);
            match fs_unmount_flow(pylon, mountpoint).await {
                Ok(()) => Response::ok_empty(req.id),
                Err(e) => Response::err(req.id, e),
            }
        },
        "mounts" => {
            drop(y);
            let all_mounts = crate::mount::list_all_mounts().await;
            let mounts: Vec<_> = all_mounts
                .iter()
                .map(|(dev, mp, fs)| serde_json::json!({"device": dev, "mountpoint": mp, "fstype": fs}))
                .collect();
            Response::ok(req.id, serde_json::json!(mounts))
        },
        "shutdown" => {
            y.shutdown().await;
            Response::ok_empty(req.id)
        },
        other => Response::err(req.id, format!("unknown command: {}", other)),
    }
}
