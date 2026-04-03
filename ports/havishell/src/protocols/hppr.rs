/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use libhavi::hppr::client::HpprdClientAsync;
use libhavi::hppr::credentials::CredentialStoreHandle;
use libhavi::protocol_handler::{
    DoneChannel, FetchContext, ProtocolHandler, Request, ResourceFetchTiming, Response,
};
use libhavi::{
    Destination, HpprDocumentSourceSnapshot, clear_hppr_document_source,
    get_hppr_document_source, set_hppr_document_source,
};

pub struct HpprHandler {
    client: Arc<HpprdClientAsync>,
    credential_store: CredentialStoreHandle,
}

impl HpprHandler {
    pub fn new(client: Arc<HpprdClientAsync>, credential_store: CredentialStoreHandle) -> Self {
        Self {
            client,
            credential_store,
        }
    }
}

impl ProtocolHandler for HpprHandler {
    fn load(
        &self,
        request: &mut Request,
        _done_chan: &mut DoneChannel,
        _context: &FetchContext,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let url = request.current_url();
        let timing_type = request.timing_type();
        let url_str = url.as_str().to_string();
        let client = self.client.clone();
        let creds = self.credential_store.clone();
        let pipeline_id = request.pipeline_id;
        let destination = request.destination;
        let reuse_source = match (pipeline_id, destination) {
            (Some(pipeline_id), dest)
                if !matches!(dest, Destination::Document | Destination::Frame | Destination::IFrame) =>
            {
                get_hppr_document_source(pipeline_id)
            },
            _ => None,
        };

        Box::pin(async move {
            let page = libhavi::pages::hppr::handle_request(
                &url_str,
                &client,
                &creds,
                reuse_source.as_ref(),
            )
            .await;
            if let Some(pipeline_id) = pipeline_id {
                match destination {
                    Destination::Document | Destination::Frame | Destination::IFrame => {
                        if let Some(source) = page.hppr_source.clone() {
                            if let Ok(address) = libhavi::hppr::url::HAVIAddress::parse(&url_str) {
                                let parts = address.parts();
                                set_hppr_document_source(
                                    pipeline_id,
                                    HpprDocumentSourceSnapshot {
                                        group: parts.group,
                                        app: parts.app,
                                        source,
                                    },
                                );
                            } else {
                                clear_hppr_document_source(pipeline_id);
                            }
                        } else {
                            clear_hppr_document_source(pipeline_id);
                        }
                    },
                    _ => {},
                }
            }
            super::page_response_to_servo(page, url, ResourceFetchTiming::new(timing_type))
        })
    }

    fn is_fetchable(&self) -> bool {
        true
    }

    fn is_secure(&self) -> bool {
        true
    }
}
