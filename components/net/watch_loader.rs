/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR WATCH streaming loader.
//!
//! Handles the network side of WATCH connections, using tokio::select!
//! for efficient cancellation and message forwarding.

use std::net::SocketAddr;
use std::sync::Arc;

use hppr_client::env_target::ViaSpec;
use hppr_client::Signer;
use ipc_channel::ipc::{IpcReceiver, IpcSender};
use ipc_channel::router::ROUTER;
use net_traits::{HpprProtocolError, WatchDomAction, WatchNetworkEvent};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::hppr_pool::{HpprAsyncState, resolve_via_to_addr};

/// Parse an error string into HpprProtocolError.
///
/// Recognizes "ERROR TYPE detail" and "FATAL TYPE detail" from HPPR protocol.
/// Falls back to CONNECTION type (fatal) for unparseable strings.
fn parse_error_string(s: &str) -> HpprProtocolError {
    if let Some(rest) = s.strip_prefix("FATAL ") {
        let (code, detail) = rest.split_once(' ').unwrap_or((rest, ""));
        HpprProtocolError { error_type: code.to_string(), detail: detail.to_string(), fatal: true }
    } else if let Some(rest) = s.strip_prefix("ERROR ") {
        let (code, detail) = rest.split_once(' ').unwrap_or((rest, ""));
        HpprProtocolError { error_type: code.to_string(), detail: detail.to_string(), fatal: false }
    } else {
        HpprProtocolError { error_type: "CONNECTION".to_string(), detail: s.to_string(), fatal: true }
    }
}

/// Unescape stream mark sequences in data.
///
/// When reading suffix mode streams, `⋯⋯🖧:` sequences must be unescaped back to `⋯🖧:`.
fn unescape_stream_mark(data: &[u8]) -> Vec<u8> {
    hppr_client::hppr_packet::writer::unescape_stream_mark(data)
}

/// Messages from DOM thread to network task.
enum DomMsg {
    Close,
}

/// Set up a listener for DOM actions, converting IPC messages to tokio channel.
fn setup_dom_listener(action_receiver: IpcReceiver<WatchDomAction>) -> UnboundedReceiver<DomMsg> {
    let (tx, rx) = unbounded_channel();
    ROUTER.add_typed_route(
        action_receiver,
        Box::new(move |msg| {
            if let Ok(action) = msg {
                match action {
                    WatchDomAction::Close => {
                        let _ = tx.send(DomMsg::Close);
                    }
                }
            }
        }),
    );
    rx
}

/// Start the WATCH streaming task.
///
/// This runs on the network thread's tokio runtime and:
/// 1. Establishes connection and starts WATCH stream
/// 2. Uses tokio::select! to handle both DOM actions and network events
/// 3. Forwards events to DOM via event_sender
pub async fn start_watch(
    _hppr_state: &Arc<HpprAsyncState>,
    endpoint: &ViaSpec,
    signer: Signer,
    urc: &str,
    event_sender: IpcSender<WatchNetworkEvent>,
    action_receiver: IpcReceiver<WatchDomAction>,
) {
    let mut dom_rx = setup_dom_listener(action_receiver);

    // Resolve endpoint to SocketAddr
    let addr = match resolve_via_to_addr(endpoint).await {
        Ok(a) => a,
        Err(e) => {
            let _ = event_sender.send(WatchNetworkEvent::Fail(parse_error_string(
                &format!("Failed to resolve endpoint: {}", e)
            )));
            return;
        }
    };

    // Create event channel for streaming - use oneshot for connection result
    let (conn_tx, conn_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
    let (events_tx, mut events_rx) = tokio::sync::mpsc::channel::<String>(32);

    // Spawn the streaming task with connection notification
    let urc_owned = urc.to_string();
    tokio::spawn(async move {
        match watch_stream_with_notify(addr, signer, urc_owned, events_tx, conn_tx).await {
            Ok(()) => {}
            Err(e) => {
                log::error!("watch stream error: {}", e);
            }
        }
    });

    // Wait for connection result before proceeding
    match conn_rx.await {
        Ok(Ok(())) => {
            // Connection successful, notify DOM
            if event_sender.send(WatchNetworkEvent::ConnectionEstablished).is_err() {
                return; // DOM dropped, stop
            }
        }
        Ok(Err(e)) => {
            // Connection failed
            let _ = event_sender.send(WatchNetworkEvent::Fail(parse_error_string(&e)));
            return;
        }
        Err(_) => {
            // Channel dropped - task panicked or was cancelled
            let _ = event_sender.send(WatchNetworkEvent::Fail(parse_error_string(
                "Connection task terminated unexpectedly"
            )));
            return;
        }
    }

    // Main loop with select!
    loop {
        tokio::select! {
            dom_msg = dom_rx.recv() => {
                match dom_msg {
                    Some(DomMsg::Close) | None => {
                        let _ = event_sender.send(WatchNetworkEvent::Close);
                        break;
                    }
                }
            }
            event = events_rx.recv() => {
                match event {
                    Some(line) => {
                        if event_sender.send(WatchNetworkEvent::Message(line)).is_err() {
                            break; // DOM dropped
                        }
                    }
                    None => {
                        // Stream closed by server
                        let _ = event_sender.send(WatchNetworkEvent::Close);
                        break;
                    }
                }
            }
        }
    }
}

/// WATCH stream with connection notification.
///
/// Notifies via `conn_tx` when connection is established (Ok) or fails (Err).
/// Then streams events to `events_tx` until the stream closes or receiver drops.
async fn watch_stream_with_notify(
    addr: SocketAddr,
    mut signer: Signer,
    urc: String,
    events_tx: tokio::sync::mpsc::Sender<String>,
    conn_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;
    use hppr_client::hppr_packet::create_null_with_headers;

    // Stream markers
    const SUFFIX_OPEN: &str = "⋯🖧: B\n";
    const SUFFIX_CLOSE_PREFIX: &str = "⋯🖧: B.";

    // Connect to server
    let stream = match TcpStream::connect(addr).await {
        Ok(s) => s,
        Err(e) => {
            let _ = conn_tx.send(Err(format!("Connection failed: {}", e)));
            return Err(format!("Connection failed: {}", e));
        }
    };

    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);

    // HELLO - all auth modes require greeting for session-id.
    // HELLO uses a Null packet, not a sealed request.
    let hello_packet = match create_null_with_headers(&[("App", "🖧HELLO")], b"") {
        Ok(p) => p,
        Err(e) => {
            let _ = conn_tx.send(Err(format!("Failed to build HELLO: {}", e)));
            return Err(format!("Failed to build HELLO: {}", e));
        }
    };
    if let Err(e) = writer.write_all(hello_packet.as_bytes()).await {
        let _ = conn_tx.send(Err(format!("Failed to send HELLO: {}", e)));
        return Err(format!("Failed to send HELLO: {}", e));
    }
    let greeting = match read_greeting(&mut reader).await {
        Ok(g) => g,
        Err(e) => {
            let _ = conn_tx.send(Err(format!("Failed to read greeting: {}", e)));
            return Err(format!("Failed to read greeting: {}", e));
        }
    };

    // Resolve signer (derives key for Ring1Adhoc)
    if let Err(e) = signer.resolve(&greeting) {
        let _ = conn_tx.send(Err(format!("Failed to resolve signer: {}", e)));
        return Err(format!("Failed to resolve signer: {}", e));
    }

    // Send WATCH command
    let watch_bytes = match signer.build_request("🖧WATCH", urc.as_bytes(), &greeting, &[]) {
        Ok(b) => b,
        Err(e) => {
            let _ = conn_tx.send(Err(format!("Failed to build WATCH: {}", e)));
            return Err(format!("Failed to build WATCH: {}", e));
        }
    };
    if let Err(e) = writer.write_all(&watch_bytes).await {
        let _ = conn_tx.send(Err(format!("Failed to send WATCH: {}", e)));
        return Err(format!("Failed to send WATCH: {}", e));
    }

    // Read stream open marker
    let mut line = String::new();
    if let Err(e) = reader.read_line(&mut line).await {
        let _ = conn_tx.send(Err(format!("Failed to read stream marker: {}", e)));
        return Err(format!("Failed to read stream marker: {}", e));
    }
    if line != SUFFIX_OPEN {
        // Check for Null packet (error response per 030-BASIC-COMMANDS.md)
        // Null packet markline: 🖧: 0.H3
        if line.starts_with("🖧: 0.") {
            let err_msg = read_null_packet_error(&mut reader, &line).await;
            let _ = conn_tx.send(Err(err_msg.clone()));
            return Err(err_msg);
        }
        // Check for direct error response (legacy fallback)
        let err_msg = if line.starts_with("FATAL ") || line.starts_with("ERROR ") {
            line.trim().to_string()
        } else {
            format!("Expected stream marker, got: {:?}", line.trim())
        };
        let _ = conn_tx.send(Err(err_msg.clone()));
        return Err(err_msg);
    }

    // Connection established successfully
    let _ = conn_tx.send(Ok(()));

    // Forward lines until stream closes or receiver drops.
    // Handles multi-blob streams: after close marker `⋯🖧: B.<hash>`,
    // check for new open marker `⋯🖧: B\n` (32 MiB boundary continuation).
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // EOF
            Ok(_) => {
                // Check for closing marker
                if line.starts_with(SUFFIX_CLOSE_PREFIX) {
                    // Check for new open marker (multi-blob continuation)
                    let mut next_line = String::new();
                    match reader.read_line(&mut next_line).await {
                        Ok(0) => break, // EOF after close - stream complete
                        Ok(_) if next_line == SUFFIX_OPEN => {
                            // New blob starting, continue reading
                            continue;
                        }
                        Ok(_) => {
                            // Unexpected content after close marker
                            log::warn!("Unexpected content after close marker: {:?}", next_line.trim());
                            break;
                        }
                        Err(e) => {
                            log::error!("WATCH stream read error after close: {}", e);
                            break;
                        }
                    }
                }
                // Check for FATAL error
                if line.starts_with("FATAL ") {
                    log::error!("WATCH stream fatal error: {}", line.trim());
                    break;
                }
                // Remove trailing newline
                if line.ends_with('\n') {
                    line.pop();
                }
                // Unescape stream marks: ⋯⋯🖧: -> ⋯🖧:
                let unescaped = unescape_stream_mark(line.as_bytes());
                // UTF-8 Lossy: WATCH events are versioned coordinates (ASCII protocol text)
                let line = String::from_utf8_lossy(&unescaped).into_owned();

                if events_tx.send(line).await.is_err() {
                    break; // Receiver dropped
                }
            }
            Err(e) => {
                log::error!("WATCH stream read error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Read error message from a Null packet response.
///
/// Null packets contain error data starting with `ERROR <TYPE> <detail>` or `FATAL <TYPE> <detail>`.
/// The markline has already been read; this reads the remaining headers and data.
async fn read_null_packet_error(
    reader: &mut tokio::io::BufReader<tokio::io::ReadHalf<tokio::net::TcpStream>>,
    markline: &str,
) -> String {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt};

    // Read headers until blank line, extract Data-Length
    let mut data_length: Option<usize> = None;
    loop {
        let mut header_line = String::new();
        match reader.read_line(&mut header_line).await {
            Ok(0) => return format!("Connection closed reading error packet (markline: {})", markline.trim()),
            Ok(_) => {
                let trimmed = header_line.trim();
                if trimmed.is_empty() {
                    // End of headers
                    break;
                }
                if let Some(len_str) = trimmed.strip_prefix("Data-Length: ") {
                    if let Ok(len) = len_str.parse::<usize>() {
                        data_length = Some(len);
                    }
                }
            }
            Err(e) => return format!("Failed to read error packet headers: {}", e),
        }
    }

    // Read data if we have a length
    let Some(len) = data_length else {
        return format!("Null packet missing Data-Length (markline: {})", markline.trim());
    };

    // Limit read to prevent DoS (error messages should be small)
    let read_len = len.min(4096);
    let mut data = vec![0u8; read_len];
    if let Err(e) = reader.read_exact(&mut data).await {
        return format!("Failed to read error packet data: {}", e);
    }

    // UTF-8 Lossy: error packets contain text per spec (ERROR/FATAL <TYPE> <detail>)
    let text = String::from_utf8_lossy(&data);
    let first_line = text.lines().next().unwrap_or("");
    if first_line.starts_with("ERROR ") || first_line.starts_with("FATAL ") {
        first_line.to_string()
    } else {
        format!("Server error: {}", first_line)
    }
}

/// Read HELLO greeting response.
async fn read_greeting(
    reader: &mut tokio::io::BufReader<tokio::io::ReadHalf<tokio::net::TcpStream>>,
) -> Result<hppr_client::Greeting, String> {
    use hppr_client::hppr_packet;
    use tokio::io::AsyncReadExt;

    let mark_bytes: [u8; 4] = [0xF0, 0x9F, 0x96, 0xA7]; // 🖧 in UTF-8
    let mut first_bytes = [0u8; 4];

    reader.read_exact(&mut first_bytes).await
        .map_err(|e| format!("Failed to read packet mark: {}", e))?;

    if first_bytes != mark_bytes {
        return Err(format!("Expected packet mark, got: {:02x?}", first_bytes));
    }

    let mut buffer = first_bytes.to_vec();
    loop {
        match hppr_packet::take_packet_from_stream(&buffer) {
            Ok((packet_ref, _rest)) => {
                // Check for error responses (Null packet with ERROR/FATAL data)
                if packet_ref.packet_type() == hppr_packet::PacketType::Null {
                    let data = packet_ref.data();
                    if data.starts_with(b"ERROR ") || data.starts_with(b"FATAL ") {
                        let msg = String::from_utf8(data.to_vec())
                            .unwrap_or_else(|_| format!("invalid error response"));
                        return Err(msg.trim().to_string());
                    }
                }

                return hppr_client::Greeting::from_packet(packet_ref)
                    .map_err(|e| format!("Failed to parse greeting: {}", e));
            }
            Err(_) => {
                let mut chunk = [0u8; 4096];
                let n = reader.read(&mut chunk).await
                    .map_err(|e| format!("Failed to read packet data: {}", e))?;
                if n == 0 {
                    return Err("Connection closed while reading packet".to_string());
                }
                buffer.extend_from_slice(&chunk[..n]);
            }
        }
    }
}
