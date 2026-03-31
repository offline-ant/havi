/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR STREAM_SUB network loader.
//!
//! Handles the network side of STREAM_SUB connections, using tokio::select!
//! for efficient cancellation.
//!
//! The repo relays trailer-format bytes on the wire. This loader parses them
//! internally with a TrailerReader and forwards:
//! - payload bytes via Data events (primary ReadableStream path)
//! - complete parsed packets via Packet events (optional onpacket diagnostics)

use std::sync::Arc;

use hppr_client::ViaSpec;
use hppr_client::Signer;
use hppr_packet::writer::TrailerReader;
use ipc_channel::ipc::{IpcReceiver, IpcSender};
use ipc_channel::router::ROUTER;
use crate::net::{HpprProtocolError, StreamSubDomAction, StreamSubNetworkEvent};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::net::hppr_pool::{HpprAsyncState, resolve_via_to_addr};

/// Parse an error string into HpprProtocolError.
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

/// Messages from DOM thread to network task.
enum DomMsg {
    Close,
}

/// Set up a listener for DOM actions, converting IPC messages to tokio channel.
fn setup_dom_listener(action_receiver: IpcReceiver<StreamSubDomAction>) -> UnboundedReceiver<DomMsg> {
    let (tx, rx) = unbounded_channel();
    ROUTER.add_typed_route(
        action_receiver,
        Box::new(move |msg| {
            if let Ok(action) = msg {
                match action {
                    StreamSubDomAction::Close => {
                        let _ = tx.send(DomMsg::Close);
                    }
                }
            }
        }),
    );
    rx
}

/// Read HELLO greeting response from a TCP stream.
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
                if packet_ref.packet_type() == hppr_packet::PacketType::Null {
                    let data = packet_ref.data();
                    if data.starts_with(b"ERROR ") || data.starts_with(b"FATAL ") {
                        let msg = String::from_utf8(data.to_vec())
                            .unwrap_or_else(|_| "invalid error response".to_string());
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

/// Start the STREAM_SUB network task.
///
/// 1. Establishes connection and sends HELLO
/// 2. Sends STREAM_SUB request (no OK response from server)
/// 3. Sends Ready event to DOM
/// 4. Enters select! loop reading bytes from TCP and forwarding Close from DOM
pub async fn start_stream_sub(
    _hppr_state: &Arc<HpprAsyncState>,
    endpoint: &ViaSpec,
    mut signer: Signer,
    prefix: &str,
    event_sender: IpcSender<StreamSubNetworkEvent>,
    action_receiver: IpcReceiver<StreamSubDomAction>,
) {
    let mut dom_rx = setup_dom_listener(action_receiver);

    // Resolve endpoint
    let addr = match resolve_via_to_addr(endpoint).await {
        Ok(a) => a,
        Err(e) => {
            let _ = event_sender.send(StreamSubNetworkEvent::Fail(parse_error_string(
                &format!("Failed to resolve endpoint: {}", e)
            )));
            return;
        }
    };

    // Connect
    let stream = match tokio::net::TcpStream::connect(addr).await {
        Ok(s) => s,
        Err(e) => {
            let _ = event_sender.send(StreamSubNetworkEvent::Fail(parse_error_string(
                &format!("Connection failed: {}", e)
            )));
            return;
        }
    };

    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = tokio::io::BufReader::new(reader);

    // HELLO
    let hello_packet = match hppr_client::hppr_packet::create_null_with_headers(&[("App", "🖧HELLO")], b"") {
        Ok(p) => p,
        Err(e) => {
            let _ = event_sender.send(StreamSubNetworkEvent::Fail(parse_error_string(
                &format!("Failed to build HELLO: {}", e)
            )));
            return;
        }
    };

    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

    if let Err(e) = writer.write_all(hello_packet.as_bytes()).await {
        let _ = event_sender.send(StreamSubNetworkEvent::Fail(parse_error_string(
            &format!("Failed to send HELLO: {}", e)
        )));
        return;
    }

    let greeting = match read_greeting(&mut reader).await {
        Ok(g) => g,
        Err(e) => {
            let _ = event_sender.send(StreamSubNetworkEvent::Fail(parse_error_string(&e)));
            return;
        }
    };

    // Resolve signer (derives key for Ring1Adhoc)
    if let Err(e) = signer.resolve(&greeting) {
        let _ = event_sender.send(StreamSubNetworkEvent::Fail(parse_error_string(
            &format!("Failed to resolve signer: {}", e)
        )));
        return;
    }

    // Build and send STREAM_SUB request
    let request_bytes = match signer.build_request("🖧STREAM_SUB", prefix.as_bytes(), &greeting, &[]) {
        Ok(b) => b,
        Err(e) => {
            let _ = event_sender.send(StreamSubNetworkEvent::Fail(parse_error_string(
                &format!("Failed to build STREAM_SUB: {}", e)
            )));
            return;
        }
    };

    if let Err(e) = writer.write_all(&request_bytes).await {
        let _ = event_sender.send(StreamSubNetworkEvent::Fail(parse_error_string(
            &format!("Failed to send STREAM_SUB: {}", e)
        )));
        return;
    }

    // No OK response for STREAM_SUB — server immediately starts relaying.
    // Signal Ready after request is sent.
    if event_sender.send(StreamSubNetworkEvent::Ready).is_err() {
        return; // DOM dropped
    }

    // Check the first line for FATAL before entering the main loop.
    // The repo may reject the subscription or the wait may time out.
    let mut pending_raw: Vec<u8> = Vec::new();
    {
        let mut first = String::new();
        match reader.read_line(&mut first).await {
            Ok(0) => {
                let _ = event_sender.send(StreamSubNetworkEvent::Fail(parse_error_string(
                    "Connection closed before data",
                )));
                return;
            }
            Err(e) => {
                let _ = event_sender.send(StreamSubNetworkEvent::Fail(parse_error_string(
                    &format!("Read failed: {}", e),
                )));
                return;
            }
            Ok(_) => {}
        }

        // Raw FATAL (repo sends this directly, not in trailer format)
        if let Some(rest) = first.strip_prefix("FATAL ") {
            let rest = rest.trim_end_matches('\n');
            let _ = event_sender.send(StreamSubNetworkEvent::Fail(parse_error_string(
                &format!("FATAL {}", rest),
            )));
            return;
        }

        // Blob trailer open wrapping a FATAL
        if first == "\u{22EF}\u{1F5A7}: B\n" {
            let mut second = String::new();
            match reader.read_line(&mut second).await {
                Ok(0) | Err(_) => {}
                Ok(_) => {
                    if let Some(rest) = second.strip_prefix("FATAL ") {
                        let rest = rest.trim_end_matches('\n');
                        let _ = event_sender.send(StreamSubNetworkEvent::Fail(
                            parse_error_string(&format!("FATAL {}", rest)),
                        ));
                        return;
                    }
                    pending_raw.extend_from_slice(second.as_bytes());
                }
            }
        }

        // Buffer initial bytes for trailer parsing
        pending_raw.splice(0..0, first.as_bytes().iter().copied());
    }

    // Main loop: read trailer bytes from TCP, parse into payload + packets
    let mut trailer_reader = TrailerReader::new();
    let mut buf = [0u8; 32768];

    // Feed any initial buffered bytes into the trailer reader
    if !pending_raw.is_empty() &&
        forward_parsed(&trailer_reader.push_lossy(&pending_raw), &event_sender).is_err()
    {
        log::debug!("stream_sub: DOM dropped during initial parse");
        return;
    }

    loop {
        tokio::select! {
            dom_msg = dom_rx.recv() => {
                match dom_msg {
                    Some(DomMsg::Close) | None => {
                        let _ = event_sender.send(StreamSubNetworkEvent::Close);
                        break;
                    }
                }
            }
            result = reader.read(&mut buf) => {
                match result {
                    Ok(0) => {
                        // EOF — publisher disconnected
                        let _ = event_sender.send(StreamSubNetworkEvent::Close);
                        break;
                    }
                    Ok(n) => {
                        let packets = trailer_reader.push_lossy(&buf[..n]);
                        if forward_parsed(&packets, &event_sender).is_err() {
                            break; // DOM dropped
                        }
                    }
                    Err(e) => {
                        let _ = event_sender.send(StreamSubNetworkEvent::Fail(parse_error_string(
                            &format!("Read failed: {}", e)
                        )));
                        break;
                    }
                }
            }
        }
    }
}

/// Send parsed packet payload bytes (Data) and complete packet bytes (Packet)
/// to the DOM. Returns Err if the DOM dropped.
fn forward_parsed(
    packets: &[hppr_packet::Packet],
    event_sender: &IpcSender<StreamSubNetworkEvent>,
) -> Result<(), ()> {
    for packet in packets {
        let payload = packet.data().to_vec();
        if !payload.is_empty() &&
            event_sender.send(StreamSubNetworkEvent::Data(payload)).is_err()
        {
            return Err(());
        }
        if event_sender
            .send(StreamSubNetworkEvent::Packet(packet.as_bytes().to_vec()))
            .is_err()
        {
            return Err(());
        }
    }
    Ok(())
}
