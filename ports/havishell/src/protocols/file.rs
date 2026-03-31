/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use libhavi::hppr::client::HpprdClientAsync;
use libhavi::hppr::credentials::CredentialStoreHandle;
use libhavi::protocol_handler::{
    DoneChannel, FetchContext, FileProtocolHander, ProtocolHandler, Request, ResourceFetchTiming,
    Response,
};

pub struct FileHpprHandler {
    client: Arc<HpprdClientAsync>,
    credential_store: CredentialStoreHandle,
    native: FileProtocolHander,
}

impl FileHpprHandler {
    pub fn new(client: Arc<HpprdClientAsync>, credential_store: CredentialStoreHandle) -> Self {
        Self {
            client,
            credential_store,
            native: FileProtocolHander::default(),
        }
    }
}

impl ProtocolHandler for FileHpprHandler {
    fn load<'a>(
        &'a self,
        request: &'a mut Request,
        done_chan: &mut DoneChannel,
        context: &FetchContext,
    ) -> Pin<Box<dyn Future<Output = Response> + Send + 'a>> {
        if !request.is_navigation_request() {
            return self.native.load(request, done_chan, context);
        }

        let url = request.current_url();
        let timing_type = request.timing_type();
        let url_str = url.as_str().to_string();
        let client = self.client.clone();
        let creds = self.credential_store.clone();

        Box::pin(async move {
            let page =
                libhavi::pages::file::handle_request(&url_str, &client, &creds).await;
            super::page_response_to_servo(page, url, ResourceFetchTiming::new(timing_type))
        })
    }

    fn is_fetchable(&self) -> bool {
        false
    }

    fn is_secure(&self) -> bool {
        true
    }
}
