/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR STREAM_IN network loader.
//!
//! Handles the network side of STREAM_IN connections, using tokio::select!
//! for efficient cancellation and data forwarding.
//!
//! When publisher_params is provided, wraps data through StreamPublisher
//! to produce trailer-format segments. Completed segments are sent back
//! to the DOM as Packet events.

use std::sync::Arc;

use hppr_client::env_target::ViaSpec;
use hppr_client::Signer;
use ipc_channel::ipc::{IpcReceiver, IpcSender};
use ipc_channel::router::ROUTER;
use net_traits::{HpprProtocolError, StreamInDomAction, StreamInNetworkEvent, StreamInPublisherParams};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::hppr_pool::{HpprAsyncState, resolve_via_to_addr};

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
    Write(Vec<u8>),
    FinishSegment,
    Close,
}

/// Set up a listener for DOM actions, converting IPC messages to tokio channel.
fn setup_dom_listener(action_receiver: IpcReceiver<StreamInDomAction>) -> UnboundedReceiver<DomMsg> {
    let (tx, rx) = unbounded_channel();
    ROUTER.add_typed_route(
        action_receiver,
        Box::new(move |msg| {
            if let Ok(action) = msg {
                match action {
                    StreamInDomAction::Write(data) => {
                        let _ = tx.send(DomMsg::Write(data));
                    }
                    StreamInDomAction::FinishSegment => {
                        let _ = tx.send(DomMsg::FinishSegment);
                    }
                    StreamInDomAction::Close => {
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

/// Read a response packet from the server after sending STREAM_IN request.
async fn read_response_packet(
    reader: &mut tokio::io::BufReader<tokio::io::ReadHalf<tokio::net::TcpStream>>,
) -> Result<Vec<u8>, String> {
    use hppr_client::hppr_packet;
    use tokio::io::AsyncReadExt;

    let mark_bytes: [u8; 4] = [0xF0, 0x9F, 0x96, 0xA7]; // 🖧 in UTF-8
    let mut first_bytes = [0u8; 4];

    reader.read_exact(&mut first_bytes).await
        .map_err(|e| format!("Failed to read response mark: {}", e))?;

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
                return Ok(packet_ref.data().to_vec());
            }
            Err(_) => {
                let mut chunk = [0u8; 4096];
                let n = reader.read(&mut chunk).await
                    .map_err(|e| format!("Failed to read response data: {}", e))?;
                if n == 0 {
                    return Err("Connection closed while reading response".to_string());
                }
                buffer.extend_from_slice(&chunk[..n]);
            }
        }
    }
}

/// Send PublisherOutput trailer bytes to TCP and packet events to DOM.
async fn send_publisher_output(
    output: hppr_segment::stream_publisher::PublisherOutput,
    writer: &mut tokio::io::WriteHalf<tokio::net::TcpStream>,
    event_sender: &IpcSender<StreamInNetworkEvent>,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;

    if !output.bytes.is_empty() {
        writer.write_all(&output.bytes).await
            .map_err(|e| format!("Write failed: {}", e))?;
    }
    for seg in &output.completed {
        // Reconstruct standard-format packet bytes from the hash.
        // PublisherOutput.completed contains hashes; we need bytes for the DOM.
        // The trailer bytes were already sent to TCP. For the Packet event,
        // send the hash string as UTF-8 bytes (DOM will receive it).
        let _ = event_sender.send(StreamInNetworkEvent::Packet(seg.hash.as_bytes().to_vec()));
    }
    Ok(())
}

/// Parse the coordinate prefix into (group, app, location) components.
///
/// Expected format: `//group/app/location` or `//group/app/location/sub`.
fn parse_coordinate(prefix: &str) -> Result<(String, String, String), String> {
    let path = prefix.strip_prefix("//").unwrap_or(prefix);
    let parts: Vec<&str> = path.splitn(3, '/').collect();
    if parts.len() < 3 {
        return Err(format!("Invalid coordinate prefix: {}", prefix));
    }
    Ok((parts[0].to_string(), parts[1].to_string(), parts[2].to_string()))
}

/// Start the STREAM_IN network task.
///
/// 1. Establishes connection and sends HELLO
/// 2. Sends STREAM_IN request, reads OK response
/// 3. Sends Ready event to DOM
/// 4. Enters select! loop forwarding Write/Close actions
///
/// In publisher mode (publisher_params provided), data goes through
/// StreamPublisher which produces trailer-format segments automatically.
pub async fn start_stream_in(
    _hppr_state: &Arc<HpprAsyncState>,
    endpoint: &ViaSpec,
    mut signer: Signer,
    prefix: &str,
    publisher_params: Option<StreamInPublisherParams>,
    event_sender: IpcSender<StreamInNetworkEvent>,
    action_receiver: IpcReceiver<StreamInDomAction>,
) {
    let mut dom_rx = setup_dom_listener(action_receiver);

    // Resolve endpoint
    let addr = match resolve_via_to_addr(endpoint).await {
        Ok(a) => a,
        Err(e) => {
            let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(
                &format!("Failed to resolve endpoint: {}", e)
            )));
            return;
        }
    };

    // Connect
    let stream = match tokio::net::TcpStream::connect(addr).await {
        Ok(s) => s,
        Err(e) => {
            let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(
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
            let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(
                &format!("Failed to build HELLO: {}", e)
            )));
            return;
        }
    };

    use tokio::io::AsyncWriteExt;

    if let Err(e) = writer.write_all(hello_packet.as_bytes()).await {
        let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(
            &format!("Failed to send HELLO: {}", e)
        )));
        return;
    }

    let greeting = match read_greeting(&mut reader).await {
        Ok(g) => g,
        Err(e) => {
            let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(&e)));
            return;
        }
    };

    // Resolve signer (derives key for Ring1Adhoc)
    if let Err(e) = signer.resolve(&greeting) {
        let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(
            &format!("Failed to resolve signer: {}", e)
        )));
        return;
    }

    // Build and send STREAM_IN request
    let request_bytes = match signer.build_request("🖧STREAM_IN", prefix.as_bytes(), &greeting, &[]) {
        Ok(b) => b,
        Err(e) => {
            let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(
                &format!("Failed to build STREAM_IN: {}", e)
            )));
            return;
        }
    };

    if let Err(e) = writer.write_all(&request_bytes).await {
        let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(
            &format!("Failed to send STREAM_IN: {}", e)
        )));
        return;
    }

    // Read OK response
    match read_response_packet(&mut reader).await {
        Ok(_data) => {}
        Err(e) => {
            let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(&e)));
            return;
        }
    }

    // Create publisher if in publisher mode
    let mut publisher = match &publisher_params {
        Some(params) => {
            let (group, app, location) = match parse_coordinate(prefix) {
                Ok(c) => c,
                Err(e) => {
                    let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(&e)));
                    return;
                }
            };
            match hppr_segment::stream_publisher::StreamPublisher::new(
                &params.key, &group, &app, &location,
                params.headers.clone(),
                params.max_segment_size,
                params.flush_seq.clone(),
            ) {
                Ok(p) => Some(p),
                Err(e) => {
                    let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(
                        &format!("Failed to create publisher: {}", e)
                    )));
                    return;
                }
            }
        }
        None => None,
    };

    // Notify DOM that server accepted
    if event_sender.send(StreamInNetworkEvent::Ready).is_err() {
        return;
    }

    // Main loop: forward Write/FinishSegment/Close actions from DOM
    loop {
        tokio::select! {
            dom_msg = dom_rx.recv() => {
                match dom_msg {
                    Some(DomMsg::Write(data)) => {
                        if let Some(ref mut pub_) = publisher {
                            match pub_.write(&data) {
                                Ok(output) => {
                                    if let Err(e) = send_publisher_output(output, &mut writer, &event_sender).await {
                                        let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(&e)));
                                        break;
                                    }
                                }
                                Err(e) => {
                                    let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(
                                        &format!("Publisher write failed: {}", e)
                                    )));
                                    break;
                                }
                            }
                        } else {
                            // Raw pipe mode
                            if let Err(e) = writer.write_all(&data).await {
                                let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(
                                    &format!("Write failed: {}", e)
                                )));
                                break;
                            }
                        }
                    }
                    Some(DomMsg::FinishSegment) => {
                        if let Some(ref mut pub_) = publisher {
                            match pub_.finish_segment() {
                                Ok(output) => {
                                    if let Err(e) = send_publisher_output(output, &mut writer, &event_sender).await {
                                        let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(&e)));
                                        break;
                                    }
                                }
                                Err(e) => {
                                    let _ = event_sender.send(StreamInNetworkEvent::Fail(parse_error_string(
                                        &format!("Publisher finish_segment failed: {}", e)
                                    )));
                                    break;
                                }
                            }
                        }
                        // In raw mode, finishSegment is a no-op
                    }
                    Some(DomMsg::Close) | None => {
                        // Close publisher (finishes any in-progress segment)
                        if let Some(ref mut pub_) = publisher {
                            match pub_.close() {
                                Ok(output) => {
                                    let _ = send_publisher_output(output, &mut writer, &event_sender).await;
                                }
                                Err(e) => {
                                    log::warn!("stream_in: publisher close failed: {}", e);
                                }
                            }
                        }
                        let _ = event_sender.send(StreamInNetworkEvent::Close);
                        break;
                    }
                }
            }
        }
    }
}
