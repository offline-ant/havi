/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::{Arc, Mutex};
use std::time::Duration;

use base::id::WebViewId;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use embedder_traits::{EmbedderMsg, EmbedderProxy};
use image::ImageEncoder;
use image::codecs::png::PngEncoder;
use malloc_size_of_derive::MallocSizeOf;
use serde_json::{Map, Value};

use crate::StreamId;
use crate::actor::{Actor, ActorError, ActorRegistry};
use crate::protocol::ClientRequest;

#[derive(MallocSizeOf)]
pub(crate) struct ScreenshotActor {
    name: String,
    #[ignore_malloc_size_of = "EmbedderProxy"]
    embedder_proxy: EmbedderProxy,
    #[ignore_malloc_size_of = "Mutex"]
    active_webview: Arc<Mutex<Option<WebViewId>>>,
}

impl ScreenshotActor {
    pub fn new(
        name: String,
        embedder_proxy: EmbedderProxy,
        active_webview: Arc<Mutex<Option<WebViewId>>>,
    ) -> Self {
        Self {
            name,
            embedder_proxy,
            active_webview,
        }
    }
}

impl Actor for ScreenshotActor {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn handle_message(
        &self,
        request: ClientRequest,
        _registry: &ActorRegistry,
        msg_type: &str,
        _msg: &Map<String, Value>,
        _id: StreamId,
    ) -> Result<(), ActorError> {
        match msg_type {
            "capture" => {
                let webview_id = {
                    let guard = self.active_webview.lock().map_err(|_| ActorError::Internal)?;
                    match *guard {
                        Some(id) => id,
                        None => {
                            let reply = serde_json::json!({
                                "from": self.name,
                                "error": "no active webview",
                            });
                            request.reply_final(&reply)?;
                            return Ok(());
                        },
                    }
                };

                let (tx, rx) = crossbeam_channel::bounded(1);
                self.embedder_proxy
                    .send(EmbedderMsg::TakeScreenshot(webview_id, tx));

                let result = rx.recv_timeout(Duration::from_secs(5));
                match result {
                    Ok(Ok(image)) => {
                        let width = image.width();
                        let height = image.height();
                        let mut png_bytes = Vec::new();
                        PngEncoder::new(&mut png_bytes)
                            .write_image(
                                image.as_raw(),
                                width,
                                height,
                                image::ExtendedColorType::Rgba8,
                            )
                            .map_err(|_| ActorError::Internal)?;
                        let b64 = BASE64.encode(&png_bytes);
                        let reply = serde_json::json!({
                            "from": self.name,
                            "data": b64,
                            "width": width,
                            "height": height,
                        });
                        request.reply_final(&reply)?;
                    },
                    Ok(Err(e)) => {
                        let reply = serde_json::json!({
                            "from": self.name,
                            "error": format!("{e:?}"),
                        });
                        request.reply_final(&reply)?;
                    },
                    Err(_) => {
                        let reply = serde_json::json!({
                            "from": self.name,
                            "error": "screenshot timeout",
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
