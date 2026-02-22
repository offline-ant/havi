/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR protocol handler wrappers for havishell.
//!
//! Each handler delegates to `havi_protocols::pages::*::handle_request()` and
//! converts the `PageResponse` to a servo `Response`.

pub mod havi;
pub mod hppr;
pub mod hppr_browse;
pub mod hppr_editor;
pub mod hppr_sandbox;
pub mod hppr_setup;

use havi_protocols::PageResponse;
use servo::protocol_handler::{HttpStatus, ResourceFetchTiming, Response, ResponseBody};
use servo::BrowserUrl;

/// Convert a `PageResponse` from havi-protocols into a servo `Response`.
fn page_response_to_servo(
    page: PageResponse,
    url: BrowserUrl,
    timing_type: ResourceFetchTiming,
) -> Response {
    let mut response = Response::new(url, timing_type);
    *response.body.lock() = ResponseBody::Done(page.body);
    response.status = HttpStatus::default(); // 200 OK
    response.headers.insert(
        "content-type",
        page.content_type
            .parse()
            .unwrap_or_else(|_| "application/octet-stream".parse().unwrap()),
    );
    if let Some((ring1_name, signing_key)) = page.admin_credentials {
        response.admin_credentials = Some((ring1_name, signing_key));
    }
    if let Some(csp) = page.csp {
        if let Ok(val) = csp.parse() {
            response.headers.insert("content-security-policy", val);
        }
    }
    response.hppr_packet = page.hppr_packet;
    response.site_credentials = page.site_credentials;
    if let Some(endpoint) = page.hppr_endpoint {
        response.hppr_endpoint = Some(endpoint);
    }
    if let Some(signer) = page.hppr_signer {
        if let Ok(parsed) = hppr_client::Signer::parse(&signer) {
            response.hppr_signer = Some(parsed);
        }
    }
    response
}
