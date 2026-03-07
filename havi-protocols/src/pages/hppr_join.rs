/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR join page handler.
//!
//! Handles hppr-join:// URLs for Ring2 membership requests.
//! URL format: hppr-join://group/app/

use std::sync::Arc;

use crate::PageResponse;
use crate::client::HpprdClientAsync;
use crate::credentials::CredentialStoreHandle;
use crate::join_fixture::get_join_fixture_state;
use crate::util::{html_escape, resolve_route_endpoint};

/// Handle an hppr-join:// URL request.
pub async fn handle_request(
    url: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    let (group, app) = match parse_join_url(url) {
        Ok(parts) => parts,
        Err(e) => return render_error(&e),
    };

    let route_cred = match credential_store
        .get_or_create_route_credential_async(&group, client)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return render_error(&format!(
                "Failed to prepare route credential for '{}': {}",
                group, e
            ));
        },
    };

    let route_sk = route_cred.signing_key().to_string();
    let route_vkey = match signing_to_verifying_key(&route_sk) {
        Ok(v) => v,
        Err(e) => return render_error(&e),
    };

    let (endpoint, _) = resolve_route_endpoint(&group, &app, client, credential_store).await;

    let fixture_state = get_join_fixture_state().as_str().to_string();
    let mut response =
        PageResponse::html(render_join_page(&group, &app, &route_vkey, &route_sk, &fixture_state));
    response.hppr_endpoint = Some(endpoint.to_string());
    response.hppr_signer = Some(format!("ring2:{}#{}", group, route_sk));
    response
}

fn parse_join_url(url: &str) -> Result<(String, String), String> {
    let rest = url
        .strip_prefix("hppr-join://")
        .ok_or("Invalid hppr-join URL: expected hppr-join://group/app/")?;

    let coord = rest.split('{').next().unwrap_or(rest);
    let mut parts = coord.split('/').filter(|s| !s.is_empty());

    let group = parts.next().unwrap_or_default().to_string();
    let app = parts.next().unwrap_or_default().to_string();

    if group.is_empty() || app.is_empty() {
        return Err("Invalid hppr-join URL: expected hppr-join://group/app/".to_string());
    }

    Ok((group, app))
}

fn signing_to_verifying_key(signing_key: &str) -> Result<String, String> {
    let (tc, sk_bytes) = hppr_packet::crypto::t_b64a_h3_decode(signing_key)
        .map_err(|e| format!("Invalid route signing key: {}", e))?;
    if tc != '&' {
        return Err("Invalid route signing key: expected '&' prefix".to_string());
    }
    hppr_packet::crypto::get_verification_key(&sk_bytes)
        .map_err(|e| format!("Failed to derive route verification key: {}", e))
}

fn render_join_page(
    group: &str,
    app: &str,
    route_vkey: &str,
    route_sk: &str,
    fixture_state: &str,
) -> String {
    let css = r#"
        body { max-width: 720px; margin: 40px auto; }
        h1 { margin-bottom: 12px; }
        .card { background: #16213e; border-radius: 8px; padding: 16px; margin: 16px 0; }
        .row { margin: 8px 0; }
        .label { color: #888; display: block; margin-bottom: 4px; }
        .mono { font-family: monospace; word-break: break-all; }
        .actions { display: flex; gap: 10px; margin-top: 12px; }
        button {
            border: 0;
            border-radius: 6px;
            cursor: pointer;
            padding: 10px 16px;
            font-size: 14px;
        }
        .primary { background: #27ae60; color: #fff; }
        .secondary { background: #444; color: #eee; }
        #status { margin-top: 12px; color: #aaa; }
        #status.error { color: #ff6b6b; }
        #status.ok { color: #4ecdc4; }
    "#;

    let body = format!(
        r#"    <h1>Join group</h1>
    <p>Request membership for <code>//{group}/{app}/</code>.</p>

    <div class="card">
        <div class="row">
            <span class="label">Group</span>
            <div class="mono">{group}</div>
        </div>
        <div class="row">
            <span class="label">Your route verification key</span>
            <div class="mono" id="route-vkey">{route_vkey}</div>
        </div>
        <div class="row">
            <span class="label">Join fixture</span>
            <div class="mono" id="join-fixture">{fixture_state}</div>
        </div>
        <div class="actions">
            <button class="secondary" id="copy-btn">Copy key</button>
            <button class="primary" id="join-btn">Request to join</button>
        </div>
        <div id="status"></div>
    </div>

    <script>
    (function() {{
        const GROUP = {group_js:?};
        const APP = {app_js:?};
        const ROUTE_VKEY = {vkey_js:?};
        const ROUTE_SK = {sk_js:?};
        const JOIN_FIXTURE = {fixture_js:?};

        const joinBtn = document.getElementById('join-btn');
        const copyBtn = document.getElementById('copy-btn');
        const statusEl = document.getElementById('status');

        function setStatus(text, cls) {{
            if (!statusEl) return;
            statusEl.textContent = text;
            statusEl.className = cls || '';
        }}

        async function pollReply(replyPath) {{
            try {{
                const packet = await window.route.get(replyPath + '|');
                const status = packet.getHeader('Request-Status');
                if (!status) return false;

                if (status === 'approved') {{
                    setStatus('Approved. Opening site...', 'ok');
                    window.address.href = 'hppr://' + GROUP + '/' + APP + '/';
                    return true;
                }}
                if (status === 'denied') {{
                    setStatus('Join request denied.', 'error');
                    return true;
                }}
                setStatus('Request status: ' + status, 'ok');
                return true;
            }} catch (_e) {{
                return false;
            }}
        }}

        async function requestJoin() {{
            if (!window.route) {{
                setStatus('window.route is unavailable for this page.', 'error');
                return;
            }}

            joinBtn.disabled = true;
            setStatus('Submitting join request...');

            if (JOIN_FIXTURE === 'pending') {{
                setStatus('Request status: pending (fixture)', 'ok');
                return;
            }}
            if (JOIN_FIXTURE === 'approved') {{
                setStatus('Approved. Opening site... (fixture)', 'ok');
                window.address.href = 'hppr://' + GROUP + '/' + APP + '/';
                return;
            }}

            try {{
                await window.route.add({{
                    headers: [
                        'Seal-By: ' + ROUTE_VKEY + ' ' + ROUTE_SK,
                        'Group: ' + GROUP,
                        'App: admin',
                        'Location: request/join'
                    ],
                    data: ''
                }});

                const replyPath = '//' + GROUP + '/admin/request/join/' + ROUTE_VKEY + '/reply/';
                setStatus('Request sent. Waiting for response...');

                if (await pollReply(replyPath)) {{
                    return;
                }}

                const watch = window.route.watch(replyPath);
                watch.onmessage = async function() {{
                    if (await pollReply(replyPath)) {{
                        watch.close();
                    }}
                }};
                watch.onerror = function() {{
                    setStatus('Watch failed. Refresh to retry.', 'error');
                }};
                watch.onclose = function() {{
                    if (!statusEl || statusEl.className !== 'ok') {{
                        joinBtn.disabled = false;
                    }}
                }};
            }} catch (e) {{
                setStatus('Join request failed: ' + (e && e.message ? e.message : String(e)), 'error');
                joinBtn.disabled = false;
            }}
        }}

        copyBtn.addEventListener('click', async function() {{
            try {{
                await navigator.clipboard.writeText(ROUTE_VKEY);
                setStatus('Route key copied.', 'ok');
            }} catch (_e) {{
                setStatus('Failed to copy route key.', 'error');
            }}
        }});

        joinBtn.addEventListener('click', requestJoin);
    }})();
    </script>"#,
        group = html_escape(group),
        app = html_escape(app),
        route_vkey = html_escape(route_vkey),
        fixture_state = html_escape(fixture_state),
        group_js = group,
        app_js = app,
        vkey_js = route_vkey,
        sk_js = route_sk,
        fixture_js = fixture_state,
    );

    crate::page_shell::render_page("Join group - HAVI", css, &body)
}

fn render_error(message: &str) -> PageResponse {
    PageResponse::error("Join Error", message, Some("Expected: <code>hppr-join://group/app/</code>"))
}
