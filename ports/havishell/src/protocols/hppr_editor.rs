/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use havi_protocols::client::HpprdClientAsync;
use havi_protocols::credentials::CredentialStoreHandle;
use servo::protocol_handler::{
    DoneChannel, FetchContext, ProtocolHandler, Request, ResourceFetchTiming, Response,
};

pub struct HpprEditorHandler {
    client: Arc<HpprdClientAsync>,
    credential_store: CredentialStoreHandle,
}

impl HpprEditorHandler {
    pub fn new(client: Arc<HpprdClientAsync>, credential_store: CredentialStoreHandle) -> Self {
        Self { client, credential_store }
    }
}

impl ProtocolHandler for HpprEditorHandler {
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

        Box::pin(async move {
            let page =
                havi_protocols::pages::hppr_editor::handle_request(&url_str, &client, &creds)
                    .await;
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
