/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR protocol handler wrappers for havishell.
//!
//! Each handler delegates to `libhavi::pages::*::handle_request()` and
//! converts the `PageResponse` to a libhavi `Response`.

pub mod file;
pub mod havi;
pub mod hppr;
pub mod hppr_browse;
pub mod hppr_sandbox;

use libhavi::hppr::PageResponse;
use libhavi::BrowserUrl;
use libhavi::protocol_handler::{HttpStatus, ResourceFetchTiming, Response, ResponseBody};

/// Convert a `PageResponse` from libhavi into a libhavi `Response`.
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
    response.hppr_lookup_trace = page.hppr_lookup_trace;
    response.hppr_content_authority = page.hppr_content_authority;
    response.hppr_source = page.hppr_source;
    response
}
