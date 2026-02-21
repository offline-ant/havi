/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::future::Future;
use std::pin::Pin;

use servo::protocol_handler::{
    DoneChannel, FetchContext, ProtocolHandler, Request, ResourceFetchTiming, Response,
};

pub struct HpprSandboxHandler;

impl HpprSandboxHandler {
    pub fn new() -> Self {
        Self
    }
}

impl ProtocolHandler for HpprSandboxHandler {
    fn load(
        &self,
        request: &mut Request,
        _done_chan: &mut DoneChannel,
        _context: &FetchContext,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let url = request.current_url();
        let timing_type = request.timing_type();
        let url_str = url.as_str().to_string();

        Box::pin(async move {
            let page = havi_protocols::pages::hppr_sandbox::handle_request(&url_str).await;
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
