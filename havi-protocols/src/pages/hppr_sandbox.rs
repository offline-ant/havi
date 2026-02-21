/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR Sandbox page handler.
//!
//! Handles hppr-sandbox: URLs for previewing remote content with strict CSP.
//! URL format: hppr-sandbox://group/app/path{via:endpoint}
//!
//! Connects as anyone to the remote repo (no credentials) and fetches
//! content with a strict Content-Security-Policy that disables scripts,
//! forms, and navigation. Used by hppr-setup:// to show a safe preview
//! before the user trusts a remote repo.

use hppr_client::env_target::parse_via;
use hppr_client::{spawn_connection, HpprRequest as IoRequest, ResponseKind, Signer};

use crate::PageResponse;
use crate::url::HAVIAddress;
use crate::util::mime_from_path;

/// Strict CSP that disables scripts, forms, and navigation.
const SANDBOX_CSP: &str = "sandbox; \
    default-src 'none'; \
    style-src 'unsafe-inline'; \
    img-src 'self' data:; \
    font-src 'self'";

/// Handle an hppr-sandbox:// URL request.
pub async fn handle_request(url: &str) -> PageResponse {
    let address = match HAVIAddress::parse(url) {
        Ok(u) => u,
        Err(e) => {
            return render_error(&e.to_string()).with_csp(SANDBOX_CSP);
        },
    };

    let endpoint_str = match address.endpoint_string() {
        Some(ep) => ep,
        None => {
            return render_error("Invalid hppr-sandbox URL: missing endpoint")
                .with_csp(SANDBOX_CSP);
        },
    };

    let endpoint = match parse_via(&endpoint_str) {
        Ok(v) => v,
        Err(e) => {
            return render_error(&format!("Invalid endpoint: {}", e))
                .with_csp(SANDBOX_CSP);
        },
    };

    let urc = address.urc_string();

    // Fetch content from remote repo anonymously
    match fetch_content(&endpoint, &urc).await {
        Ok((content_type, location, body)) => {
            let location_hint = if location.is_empty() { &urc } else { &location };
            let mime = if content_type.is_empty() {
                mime_from_path(location_hint)
            } else {
                &content_type
            };
            PageResponse::new(mime.to_string(), body).with_csp(SANDBOX_CSP)
        },
        Err(e) => {
            render_error(&e).with_csp(SANDBOX_CSP)
        },
    }
}

/// Fetch content from remote repo using anyone authentication via a new connection.
async fn fetch_content(
    endpoint: &hppr_client::env_target::ViaSpec,
    urc: &str,
) -> Result<(String, String, Vec<u8>), String> {
    // Parse endpoint to socket address
    let addr = match endpoint {
        hppr_client::env_target::ViaSpec::Net { host, port, .. } => {
            format!("{}:{}", host, port)
                .parse::<std::net::SocketAddr>()
                .map_err(|e| format!("Invalid address: {}", e))?
        },
        _ => return Err("Unsupported transport for sandbox".to_string()),
    };

    let conn = spawn_connection(addr, Signer::anyone())
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    let resp = conn
        .send(IoRequest::Get { urc: urc.to_string() })
        .await
        .map_err(|e| format!("GET failed: {}", e))?;

    match resp.kind {
        ResponseKind::Packet(packet) => {
            let content_type = packet
                .header("Content-Type")
                .unwrap_or("")
                .to_string();
            let location = packet.header("Location").unwrap_or("").to_string();
            let body = packet.data().to_vec();
            Ok((content_type, location, body))
        },
        _ => Err("Unexpected response type".to_string()),
    }
}

/// Render error page.
fn render_error(message: &str) -> PageResponse {
    PageResponse::error(
        "Preview Error",
        message,
        Some("Expected URL format: <code>hppr-sandbox://group/app/path{via:192.168.1.10}</code>"),
    )
}

#[cfg(test)]
mod tests {
    use crate::url::HAVIAddress;

    #[test]
    fn test_parse_sandbox_url_via_hppr_url() {
        let url = HAVIAddress::parse("hppr-sandbox://chess/game/{via:192.168.1.10}").unwrap();
        assert_eq!(url.endpoint_string(), Some("192.168.1.10:4777".to_string()));
        assert_eq!(url.urc_string(), "//chess/game/");

        let url = HAVIAddress::parse("hppr-sandbox://mygroup/app/index.html{via:10.0.0.5:4778}").unwrap();
        assert_eq!(url.endpoint_string(), Some("10.0.0.5:4778".to_string()));
        assert_eq!(url.urc_string(), "//mygroup/app/index.html");
    }

    #[test]
    fn test_parse_sandbox_url_errors() {
        assert!(HAVIAddress::parse("hppr-sandbox://chess/game/").is_err());
        assert!(HAVIAddress::parse("hppr-sandbox:").is_err());
    }
}
