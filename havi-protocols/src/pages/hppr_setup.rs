/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain name at https://mozilla.org/MPL/2.0/. */

//! HPPR Setup page handler.
//!
//! Handles hppr-setup: URLs for trust establishment with remote repos.
//! URL format: hppr-setup://group/app/location{via:endpoint}
//!
//! Serves a static HTML page that:
//! 1. Creates anyone client via HpprClient.connect() and calls hello() to get the repo's verification key
//! 2. Shows UI to accept/trust that key
//! 3. Stores route and site-trust packets via ring0
//! 4. Displays a sandboxed preview using the `<x>` xframe element

use std::sync::Arc;

use crate::PageResponse;
use crate::client::{get_admin_credentials, HpprdClientAsync};
use crate::credentials::CredentialStoreHandle;
use crate::url::HAVIAddress;
use crate::util::html_escape;

/// Handle an hppr-setup:// URL request.
pub async fn handle_request(
    url: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    let address = match HAVIAddress::parse(url) {
        Ok(u) => u,
        Err(e) => {
            return render_error(&e.to_string());
        },
    };

    let endpoint = address.endpoint_string().unwrap_or_default();
    let parts = address.parts();
    let location = if parts.location.is_empty() {
        "index.html".to_string()
    } else {
        parts.location.clone()
    };

    if endpoint.is_empty() {
        return render_error("Invalid hppr-setup URL: missing endpoint");
    }

    if parts.group.is_empty() {
        return render_error("Invalid hppr-setup URL: missing group");
    }

    // Ensure per-group route key exists for setup actions.
    let _ = credential_store
        .get_or_create_route_credential_async(&parts.group, client)
        .await;

    let (ring1_name, token) = get_admin_credentials();
    let html = render_setup_page(&endpoint, &parts.group, &parts.app, &location);
    PageResponse::html(html).with_admin_credentials(ring1_name, token)
}

/// Render the setup page HTML.
fn render_setup_page(endpoint: &str, group: &str, app: &str, location: &str) -> String {
    let coord = if app.is_empty() {
        format!("//{}/", html_escape(group))
    } else {
        format!("//{}/{}/", html_escape(group), html_escape(app))
    };

    let html = format!(
        r##"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>{coord} - HAVI</title>
    <style>
        * {{ box-sizing: border-box; margin: 0; padding: 0; }}
        html, body {{ height: 100%; font-family: system-ui, sans-serif; background: #1a1a2e; color: #eee; }}
        body {{ display: flex; flex-direction: column; }}

        details {{ background: #16213e; border-bottom: 1px solid #333; }}
        summary {{
            padding: 8px 16px; cursor: pointer; font-size: 0.9em;
            display: flex; align-items: center; gap: 12px;
            color: #888; user-select: none;
        }}
        summary::-webkit-details-marker {{ display: none; }}
        summary::before {{ content: '\25B6'; font-size: 0.7em; transition: transform 0.15s; }}
        details[open] > summary::before {{ transform: rotate(90deg); }}
        summary .coord {{ color: #4ecdc4; font-family: monospace; }}
        summary .endpoint {{ color: #7fdbff; font-family: monospace; }}

        .panel {{ padding: 12px 16px; max-width: 640px; }}
        .card {{ background: #0f1729; border-radius: 6px; padding: 12px; margin-bottom: 10px; }}
        .section-title {{ color: #4ecdc4; font-size: 0.8em; margin-bottom: 8px; text-transform: uppercase; letter-spacing: 1px; }}
        .info-row {{ display: flex; justify-content: space-between; padding: 6px 0; border-bottom: 1px solid #222; font-size: 0.85em; }}
        .info-row:last-child {{ border-bottom: none; }}
        .info-label {{ color: #888; }}
        .info-value {{ font-family: monospace; color: #7fdbff; word-break: break-all; }}
        .key-small {{ font-family: monospace; font-size: 0.8em; color: #7fdbff; word-break: break-all; }}

        .diff-indicator {{ font-size: 0.7em; padding: 1px 5px; border-radius: 3px; margin-left: 6px; }}
        .diff-new {{ background: #27ae60; color: #fff; }}
        .diff-changed {{ background: #f39c12; color: #000; }}
        .diff-same {{ background: #555; color: #ccc; }}

        .checkbox-row {{ display: flex; align-items: flex-start; padding: 6px 0; }}
        .checkbox-row input[type="checkbox"] {{ margin-right: 8px; margin-top: 3px; }}
        .checkbox-label {{ flex: 1; }}
        .checkbox-label .title {{ color: #eee; font-size: 0.85em; }}
        .checkbox-label .desc {{ color: #888; font-size: 0.8em; margin-top: 1px; }}

        .buttons {{ display: flex; gap: 10px; margin-top: 10px; }}
        button {{ padding: 8px 20px; font-size: 0.9em; border: none; border-radius: 4px; cursor: pointer; }}
        button.primary {{ background: #27ae60; color: white; }}
        button.primary:hover {{ background: #2ecc71; }}
        button.primary:disabled {{ background: #555; cursor: not-allowed; }}
        button.secondary {{ background: #444; color: #eee; }}
        button.secondary:hover {{ background: #555; }}

        .warning {{ background: #2d2a1f; border-left: 3px solid #f39c12; padding: 8px 12px; font-size: 0.8em; color: #f1c40f; margin-top: 8px; }}
        #error {{ display: none; color: #ff6b6b; background: #2d1f1f; border-left: 3px solid #ff6b6b; padding: 10px 12px; margin-bottom: 8px; font-size: 0.85em; }}
        #loading {{ text-align: center; padding: 20px; color: #888; font-size: 0.85em; }}
        #content {{ display: none; }}

        #preview-frame {{ flex: 1; width: 100%; border: none; background: #fff; display: block; }}
    </style>
</head>
<body>
    <details>
        <summary>
            Trust Setup &mdash;
            <span class="endpoint">{endpoint}</span>
            <span class="coord">{coord}</span>
        </summary>
        <div class="panel">
            <div id="error"></div>
            <div id="loading"><p>Connecting to {endpoint}...</p></div>
            <div id="content">
                <div class="card">
                    <div class="section-title">Remote Repo</div>
                    <div class="info-row">
                        <span class="info-label">Upstream</span>
                        <span class="info-value">{endpoint}</span>
                    </div>
                    <div class="info-row">
                        <span class="info-label">Coordinate</span>
                        <span class="info-value">{coord}</span>
                    </div>
                    <div class="info-row">
                        <span class="info-label">Repo ID</span>
                        <span class="info-value" id="repo-id"></span>
                    </div>
                    <div class="info-row">
                        <span class="info-label">Repo Key</span>
                        <span class="info-value key-small" id="repo-key"></span>
                    </div>
                    <div class="info-row">
                        <span class="info-label">Site Trust</span>
                        <span class="info-value key-small" id="remote-trust-keys"></span>
                    </div>
                </div>

                <div class="card" id="local-state" style="display: none;">
                    <div class="section-title">Current Home Repo Route</div>
                    <div class="info-row">
                        <span class="info-label">Upstream</span>
                        <span class="info-value" id="local-endpoint"></span>
                    </div>
                    <div class="info-row">
                        <span class="info-label">Site Trust</span>
                        <span class="info-value key-small" id="local-trust-keys"></span>
                    </div>
                </div>

                <div class="card">
                    <div class="section-title">Actions</div>
                    <div class="checkbox-row">
                        <input type="checkbox" id="adopt-admin-key" checked>
                        <div class="checkbox-label">
                            <div class="title">Adopt Site Trust<span id="trust-diff" class="diff-indicator"></span></div>
                            <div class="desc">Use the remote repo's trusted keys for content verification</div>
                        </div>
                    </div>
                    <div class="checkbox-row">
                        <input type="checkbox" id="set-endpoint" checked>
                        <div class="checkbox-label">
                            <div class="title">Set default endpoint<span id="endpoint-diff" class="diff-indicator"></span></div>
                            <div class="desc">Use this repo as the default for {coord}</div>
                        </div>
                    </div>
                </div>

                <div class="warning">
                    Only accept keys from repos you trust.
                </div>

                <div class="buttons">
                    <button class="secondary" onclick="cancel()">Cancel</button>
                    <button class="primary" id="accept-btn" onclick="accept()">Accept &amp; Trust</button>
                </div>
            </div>
        </div>
    </details>

    <x id="preview-frame"></x>

    <script>
        var GROUP = '{group_js}';
        var APP = '{app_js}';
        var ENDPOINT = '{endpoint_js}';
        var LOCATION = '{location_js}';
    </script>
</body>
</html>"##,
        endpoint = html_escape(endpoint),
        coord = coord,
        group_js = html_escape(group),
        app_js = html_escape(app),
        endpoint_js = html_escape(endpoint),
        location_js = html_escape(location),
    );

    // Inject the external JS before the closing </body> tag
    let setup_js = include_str!("../js/hppr-setup.js");
    html.replace(
        "</body>",
        &format!("    <script>{}</script>\n</body>", setup_js),
    )
}

/// Render error page.
fn render_error(message: &str) -> PageResponse {
    PageResponse::error(
        "Setup Error",
        message,
        Some("Expected: <code>hppr-setup://group/app/location{via:192.168.1.10}</code>"),
    )
}
