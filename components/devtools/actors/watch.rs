/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::{Arc, Mutex};
use std::time::Duration;

use embedder_traits::{EmbedderMsg, EmbedderProxy};
use malloc_size_of_derive::MallocSizeOf;
use serde_json::{Map, Value};

use crate::StreamId;
use crate::actor::{Actor, ActorError, ActorRegistry};
use crate::protocol::ClientRequest;

#[derive(MallocSizeOf)]
pub(crate) struct WatchActor {
    name: String,
    #[ignore_malloc_size_of = "EmbedderProxy"]
    embedder_proxy: EmbedderProxy,
    #[ignore_malloc_size_of = "Mutex"]
    mode: Arc<Mutex<String>>,
}

impl WatchActor {
    pub fn new(name: String, embedder_proxy: EmbedderProxy, mode: Arc<Mutex<String>>) -> Self {
        Self {
            name,
            embedder_proxy,
            mode,
        }
    }

    fn is_valid_mode(mode: &str) -> bool {
        matches!(mode, "off" | "notify" | "auto" | "dev")
    }
}

impl Actor for WatchActor {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn handle_message(
        &self,
        request: ClientRequest,
        _registry: &ActorRegistry,
        msg_type: &str,
        msg: &Map<String, Value>,
        _id: StreamId,
    ) -> Result<(), ActorError> {
        match msg_type {
            "getMode" => {
                let (tx, rx) = crossbeam_channel::bounded(1);
                self.embedder_proxy.send(EmbedderMsg::WatchGetMode(tx));

                match rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(mode) => {
                        if let Ok(mut guard) = self.mode.lock() {
                            *guard = mode.clone();
                        }
                        let reply = serde_json::json!({
                            "from": self.name,
                            "mode": mode,
                        });
                        request.reply_final(&reply)?;
                    },
                    Err(_) => {
                        let cached = self
                            .mode
                            .lock()
                            .map(|guard| guard.clone())
                            .unwrap_or_else(|_| "off".to_string());
                        let reply = serde_json::json!({
                            "from": self.name,
                            "mode": cached,
                        });
                        request.reply_final(&reply)?;
                    },
                }
            },
            "setMode" => {
                let mode = msg
                    .get("mode")
                    .ok_or(ActorError::MissingParameter)?
                    .as_str()
                    .ok_or(ActorError::BadParameterType)?;

                if !Self::is_valid_mode(mode) {
                    let reply = serde_json::json!({
                        "from": self.name,
                        "error": "invalid mode",
                    });
                    request.reply_final(&reply)?;
                    return Ok(());
                }

                let (tx, rx) = crossbeam_channel::bounded(1);
                self.embedder_proxy
                    .send(EmbedderMsg::WatchSetMode(mode.to_string(), tx));

                match rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(mode) => {
                        if let Ok(mut guard) = self.mode.lock() {
                            *guard = mode.clone();
                        }
                        let reply = serde_json::json!({
                            "from": self.name,
                            "mode": mode,
                        });
                        request.reply_final(&reply)?;
                    },
                    Err(_) => {
                        let reply = serde_json::json!({
                            "from": self.name,
                            "error": "watch timeout",
                        });
                        request.reply_final(&reply)?;
                    },
                }
            },
            _ => return Err(ActorError::UnrecognizedPacketType),
        }
        Ok(())
    }
}
