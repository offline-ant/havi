//! TCP control server. Accepts JSON lines connections.
//!
//! Tracks connected clients. When no clients are connected for
//! `IDLE_TIMEOUT`, triggers automatic shutdown.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};

use tokio::sync::mpsc;

use crate::protocol::{Event, Request, Response};
use crate::Yard;

/// Idle timeout: shut down if no clients connect within this duration.
const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Run the control server on the given listener.
pub async fn run(
    listener: TcpListener,
    yard: Arc<Mutex<Yard>>,
    mut yard_events: mpsc::UnboundedReceiver<crate::service::ServiceEvent>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let (event_tx, _) = broadcast::channel::<String>(64);
    let client_count = Arc::new(AtomicUsize::new(0));

    // Forward service events to broadcast channel
    let event_tx2 = event_tx.clone();
    tokio::spawn(async move {
        while let Some(svc_event) = yard_events.recv().await {
            let event = Event {
                event: match svc_event.state {
                    crate::service::State::Running => "service_started".to_string(),
                    crate::service::State::Stopped => "service_stopped".to_string(),
                    _ => format!("service_{}", serde_json::to_value(svc_event.state).unwrap_or_default()),
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
    let idle_yard = Arc::clone(&yard);
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
                    let mut y = idle_yard.lock().await;
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
                        let yard = Arc::clone(&yard);
                        let event_rx = event_tx.subscribe();
                        let cc = Arc::clone(&client_count);
                        tokio::spawn(async move {
                            handle_client(stream, yard, event_rx).await;
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
    yard: Arc<Mutex<Yard>>,
    mut event_rx: broadcast::Receiver<String>,
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
            }
        };

        let resp = dispatch(&yard, req).await;
        let msg = serde_json::to_string(&resp).unwrap_or_default() + "\n";
        if write_tx.send(msg).is_err() {
            break;
        }
    }

    drop(write_tx);
    let _ = write_handle.await;
}

async fn dispatch(yard: &Arc<Mutex<Yard>>, req: Request) -> Response {
    let mut y = yard.lock().await;
    match req.cmd.as_str() {
        "status" => {
            let data = y.status();
            Response::ok(req.id, data)
        }
        "list" => {
            Response::ok(req.id, serde_json::json!({
                "services": crate::services::SERVICES,
            }))
        }
        "start" => {
            let Some(name) = req.service.as_deref() else {
                return Response::err(req.id, "missing 'service' field");
            };
            match y.start_service(name, &req.args).await {
                Ok(()) => Response::ok_empty(req.id),
                Err(e) => Response::err(req.id, e),
            }
        }
        "stop" => {
            let Some(name) = req.service.as_deref() else {
                return Response::err(req.id, "missing 'service' field");
            };
            match y.stop_service(name).await {
                Ok(_) => Response::ok_empty(req.id),
                Err(e) => Response::err(req.id, e),
            }
        }
        "shutdown" => {
            y.shutdown().await;
            Response::ok_empty(req.id)
        }
        other => Response::err(req.id, format!("unknown command: {}", other)),
    }
}
