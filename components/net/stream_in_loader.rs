/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR STREAM_IN network loader.
//!
//! Thin bridge between DOM IPC and `AsyncStreamInSession` from the hppr
//! client library.  Connects via `connect_via` (supports TCP, QUIB,
//! WebSocket, Unix), then forwards Write/FinishSegment/Close actions.
//! Completed packets are delivered synchronously by the session's
//! `on_packet` callback and forwarded to the DOM before close.

use std::sync::Arc;

use hppr_client::Signer;
use hppr_client::ViaSpec;
use hppr_client::tokio::connect_via;
use ipc_channel::ipc::{IpcReceiver, IpcSender};
use ipc_channel::router::ROUTER;
use net_traits::{HpprProtocolError, StreamInDomAction, StreamInNetworkEvent, StreamInPublisherParams};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::hppr_pool::HpprAsyncState;

/// Parse an error string into HpprProtocolError.
fn error_from(s: &str) -> HpprProtocolError {
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
                    StreamInDomAction::Write(data) => { let _ = tx.send(DomMsg::Write(data)); }
                    StreamInDomAction::FinishSegment => { let _ = tx.send(DomMsg::FinishSegment); }
                    StreamInDomAction::Close => { let _ = tx.send(DomMsg::Close); }
                }
            }
        }),
    );
    rx
}

/// Build `hppr_client::StreamInOptions` from DOM publisher params.
fn stream_in_options(params: &StreamInPublisherParams) -> hppr_client::StreamInOptions {
    let mut opts = hppr_client::StreamInOptions::new(&params.key);
    if !params.headers.is_empty() {
        opts = opts.headers(params.headers.clone());
    }
    if let Some(max) = params.max_segment_size {
        opts = opts.max_segment_size(max);
    }
    if let Some(ref seq) = params.flush_seq {
        opts = opts.flush_sequence(seq.clone());
    }
    opts
}

/// Send completed packets to the DOM via IPC.
///
/// Returns false if the DOM dropped the channel.
fn send_packet(event_sender: &IpcSender<StreamInNetworkEvent>, packet: &hppr_packet::Packet) -> bool {
    event_sender
        .send(StreamInNetworkEvent::Packet(packet.as_bytes().to_vec()))
        .is_ok()
}

/// Start the STREAM_IN network task.
///
/// 1. Connects via `connect_via` (TCP, QUIB, WS, Unix)
/// 2. Opens `AsyncStreamInSession` (handles HELLO + STREAM_IN internally)
/// 3. Sends Ready event to DOM
/// 4. Forwards Write/FinishSegment/Close actions from DOM
/// 5. Delivers completed packets via `on_packet` callback before close
pub async fn start_stream_in(
    _hppr_state: &Arc<HpprAsyncState>,
    endpoint: &ViaSpec,
    signer: Signer,
    prefix: &str,
    publisher_params: StreamInPublisherParams,
    event_sender: IpcSender<StreamInNetworkEvent>,
    action_receiver: IpcReceiver<StreamInDomAction>,
) {
    let mut dom_rx = setup_dom_listener(action_receiver);
    let options = stream_in_options(&publisher_params);

    // Connect and open session (HELLO + STREAM_IN handled by client lib)
    let connection = match connect_via(endpoint, signer).await {
        Ok(c) => c,
        Err(e) => {
            let _ = event_sender.send(StreamInNetworkEvent::Fail(error_from(&e.to_string())));
            return;
        }
    };

    let mut session = match connection.stream_in(prefix, options).await {
        Ok(s) => s,
        Err(e) => {
            let _ = event_sender.send(StreamInNetworkEvent::Fail(error_from(&e.to_string())));
            return;
        }
    };

    // Notify DOM that server accepted
    if event_sender.send(StreamInNetworkEvent::Ready).is_err() {
        return;
    }

    // Packet callback: forward completed packets to DOM via IPC
    let es = event_sender.clone();
    let mut on_packet = move |packet: hppr_packet::Packet| {
        send_packet(&es, &packet);
    };

    // Main loop: forward DOM actions to session
    loop {
        match dom_rx.recv().await {
            Some(DomMsg::Write(data)) => {
                if let Err(e) = session.write(&data, &mut on_packet).await {
                    let _ = event_sender.send(StreamInNetworkEvent::Fail(error_from(&e.to_string())));
                    break;
                }
            }
            Some(DomMsg::FinishSegment) => {
                if let Err(e) = session.flush(&mut on_packet).await {
                    let _ = event_sender.send(StreamInNetworkEvent::Fail(error_from(&e.to_string())));
                    break;
                }
            }
            Some(DomMsg::Close) | None => {
                // close() consumes session — packets delivered via on_packet
                // before the session shuts down.
                if let Err(e) = session.close(&mut on_packet).await {
                    log::warn!("stream_in: close failed: {}", e);
                }
                let _ = event_sender.send(StreamInNetworkEvent::Close);
                break;
            }
        }
    }
}
