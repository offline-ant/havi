/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::{Arc, Mutex};
use std::time::Duration;

use base::id::WebViewId;
use embedder_traits::{EmbedderMsg, EmbedderProxy};
use malloc_size_of_derive::MallocSizeOf;
use rustc_hash::FxHashMap;
use serde_json::{Map, Value};

use super::StreamId;
use super::actor::{Actor, ActorError, ActorRegistry};
use super::actors::root::RootActor;
use super::protocol::ClientRequest;

#[derive(MallocSizeOf)]
pub(crate) struct ShellActor {
    name: String,
    #[ignore_malloc_size_of = "EmbedderProxy"]
    embedder_proxy: EmbedderProxy,
    #[ignore_malloc_size_of = "Mutex"]
    active_webview: Arc<Mutex<Option<WebViewId>>>,
    #[ignore_malloc_size_of = "Mutex"]
    webviews_by_browser_id: Arc<Mutex<FxHashMap<u32, WebViewId>>>,
}

impl ShellActor {
    pub fn new(
        name: String,
        embedder_proxy: EmbedderProxy,
        active_webview: Arc<Mutex<Option<WebViewId>>>,
        webviews_by_browser_id: Arc<Mutex<FxHashMap<u32, WebViewId>>>,
    ) -> Self {
        Self {
            name,
            embedder_proxy,
            active_webview,
            webviews_by_browser_id,
        }
    }

    fn active_browser_id(&self) -> Option<u32> {
        let active = self.active_webview.lock().ok()?.to_owned()?;
        let map = self.webviews_by_browser_id.lock().ok()?;
        map.iter()
            .find_map(|(browser_id, webview_id)| (*webview_id == active).then_some(*browser_id))
    }

    fn resolve_webview_id(&self, msg: &Map<String, Value>) -> Result<WebViewId, ActorError> {
        if let Some(browser_id) = msg.get("browserId") {
            let browser_id = browser_id
                .as_u64()
                .ok_or(ActorError::BadParameterType)? as u32;
            let guard = self
                .webviews_by_browser_id
                .lock()
                .map_err(|_| ActorError::Internal)?;
            return guard
                .get(&browser_id)
                .copied()
                .ok_or(ActorError::MissingParameter);
        }

        let guard = self.active_webview.lock().map_err(|_| ActorError::Internal)?;
        guard.ok_or(ActorError::MissingParameter)
    }
}

impl Actor for ShellActor {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn handle_message(
        &self,
        request: ClientRequest,
        registry: &ActorRegistry,
        msg_type: &str,
        msg: &Map<String, Value>,
        _id: StreamId,
    ) -> Result<(), ActorError> {
        match msg_type {
            "getActiveBrowserId" => {
                let reply = serde_json::json!({
                    "from": self.name,
                    "browserId": self.active_browser_id(),
                });
                request.reply_final(&reply)?;
            }
            "setUrl" => {
                let webview_id = match self.resolve_webview_id(msg) {
                    Ok(id) => id,
                    Err(_) => {
                        let reply = serde_json::json!({
                            "from": self.name,
                            "error": "unknown browserId",
                        });
                        request.reply_final(&reply)?;
                        return Ok(());
                    }
                };

                let url = msg
                    .get("url")
                    .ok_or(ActorError::MissingParameter)?
                    .as_str()
                    .ok_or(ActorError::BadParameterType)?
                    .to_string();

                let (tx, rx) = crossbeam_channel::bounded(1);
                self.embedder_proxy
                    .send(EmbedderMsg::DevtoolsSetUrl(webview_id, url.clone(), tx));

                match rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(Ok(effective_url)) => {
                        let reply = serde_json::json!({
                            "from": self.name,
                            "ok": true,
                            "url": effective_url,
                        });
                        request.reply_final(&reply)?;
                    }
                    Ok(Err(detail)) => {
                        let reply = serde_json::json!({
                            "from": self.name,
                            "error": detail,
                        });
                        request.reply_final(&reply)?;
                    }
                    Err(_) => {
                        let reply = serde_json::json!({
                            "from": self.name,
                            "error": "shell timeout",
                        });
                        request.reply_final(&reply)?;
                    }
                }
            }
            "activateTab" => {
                let webview_id = match self.resolve_webview_id(msg) {
                    Ok(id) => id,
                    Err(_) => {
                        let reply = serde_json::json!({
                            "from": self.name,
                            "error": "unknown browserId",
                        });
                        request.reply_final(&reply)?;
                        return Ok(());
                    }
                };

                let (tx, rx) = crossbeam_channel::bounded(1);
                self.embedder_proxy
                    .send(EmbedderMsg::DevtoolsActivateWebView(webview_id, tx));

                match rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(Ok(())) => {
                        if let Ok(mut active) = self.active_webview.lock() {
                            *active = Some(webview_id);
                        }
                        if let Some(active_browser_id) = self.active_browser_id() {
                            let root = registry.find::<RootActor>("root");
                            root.set_active_tab_by_browser_id(registry, active_browser_id);
                        }
                        let reply = serde_json::json!({
                            "from": self.name,
                            "ok": true,
                        });
                        request.reply_final(&reply)?;
                    }
                    Ok(Err(detail)) => {
                        let reply = serde_json::json!({
                            "from": self.name,
                            "error": detail,
                        });
                        request.reply_final(&reply)?;
                    }
                    Err(_) => {
                        let reply = serde_json::json!({
                            "from": self.name,
                            "error": "shell timeout",
                        });
                        request.reply_final(&reply)?;
                    }
                }
            }
            _ => return Err(ActorError::UnrecognizedPacketType),
        }
        Ok(())
    }
}
