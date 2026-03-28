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
use crate::url::HAVIAddress;
use crate::util::{html_escape, resolve_route_endpoint, signing_to_verifying_key};

/// Handle an hppr-join:// URL request.
pub async fn handle_request(
    url: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    let address = match HAVIAddress::parse(url) {
        Ok(a) => a,
        Err(e) => return render_error(&e.to_string()),
    };
    let parts = address.parts();
    let (group, app) = (parts.group, parts.app);
    if group.is_empty() || app.is_empty() {
        return render_error("Invalid hppr-join URL: expected hppr-join://group/app/");
    }

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

    let (endpoint, _, _, _) = resolve_route_endpoint(&group, &app, client, credential_store).await;

    let fixture_state = get_join_fixture_state().as_str().to_string();
    let mut response =
        PageResponse::html(render_join_page(&group, &app, &route_vkey, &route_sk, &fixture_state));
    response.hppr_endpoint = Some(endpoint.to_string());
    response.hppr_signer = Some(format!("ring2:{}#{}", group, route_sk));
    response
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
        input[type="text"], input[type="password"] {
            background: #1a1a2e;
            border: 1px solid #444;
            border-radius: 4px;
            color: #eee;
            padding: 8px 10px;
            font-size: 14px;
            width: 100%;
            box-sizing: border-box;
        }
        input:focus { border-color: #27ae60; outline: none; }
        .separator {
            text-align: center;
            color: #666;
            margin: 24px 0 8px;
            font-size: 13px;
        }
        .separator span {
            background: #0f0f23;
            padding: 0 12px;
        }
        .separator hr {
            border: none;
            border-top: 1px solid #333;
            margin-top: -8px;
        }
        .status { margin-top: 12px; color: #aaa; }
        .status.error { color: #ff6b6b; }
        .status.ok { color: #4ecdc4; }
    "#;

    let body = format!(
        r#"    <h1>Join group</h1>
    <p>Access to <code>//{group}/{app}/</code> requires group membership.</p>

    <div class="card">
        <div class="row">
            <span class="label">Username</span>
            <input type="text" id="login-user" autocomplete="username">
        </div>
        <div class="row">
            <span class="label">Password</span>
            <input type="password" id="login-pass" autocomplete="current-password">
        </div>
        <div class="actions">
            <button class="primary" id="login-btn">Log in</button>
        </div>
        <div id="login-status" class="status"></div>
    </div>

    <div class="separator"><span>or request to join</span><hr></div>

    <div class="card">
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
        <div id="join-status" class="status"></div>
    </div>

    <script>
    (function() {{
        const GROUP = {group_js:?};
        const APP = {app_js:?};
        const ROUTE_VKEY = {vkey_js:?};
        const ROUTE_SK = {sk_js:?};
        const JOIN_FIXTURE = {fixture_js:?};
        const ENDPOINT = window.route ? window.route.endpoint : '';

        const loginBtn = document.getElementById('login-btn');
        const loginUser = document.getElementById('login-user');
        const loginPass = document.getElementById('login-pass');
        const loginStatus = document.getElementById('login-status');

        const joinBtn = document.getElementById('join-btn');
        const copyBtn = document.getElementById('copy-btn');
        const joinStatus = document.getElementById('join-status');

        function setStatus(el, text, cls) {{
            if (!el) return;
            el.textContent = text;
            el.className = 'status ' + (cls || '');
        }}

        // --- Password login ---

        async function passwordLogin() {{
            const username = loginUser.value.trim();
            const password = loginPass.value;

            if (!username) {{
                setStatus(loginStatus, 'Username is required.', 'error');
                return;
            }}
            if (!password) {{
                setStatus(loginStatus, 'Password is required.', 'error');
                return;
            }}
            if (!ENDPOINT) {{
                setStatus(loginStatus, 'No route endpoint available.', 'error');
                return;
            }}

            loginBtn.disabled = true;
            setStatus(loginStatus, 'Connecting...');

            try {{
                const client = await HpprClient.connectRing2Password(
                    ENDPOINT, GROUP, username, password
                );
                // Probe: try HELLO to verify the derived key is accepted.
                await client.hello();
                setStatus(loginStatus, 'Authenticated. Opening site...', 'ok');
                window.address.href = 'hppr://' + GROUP + '/' + APP + '/';
            }} catch (e) {{
                const msg = e && e.message ? e.message : String(e);
                if (msg.includes('UNAUTHORIZED') || msg.includes('not a member')) {{
                    setStatus(loginStatus, 'Login failed: not a member of this group.', 'error');
                }} else {{
                    setStatus(loginStatus, 'Login failed: ' + msg, 'error');
                }}
                loginBtn.disabled = false;
            }}
        }}

        loginBtn.addEventListener('click', passwordLogin);
        loginPass.addEventListener('keydown', function(e) {{
            if (e.key === 'Enter') passwordLogin();
        }});

        // --- Join request ---

        async function pollReply(replyPath) {{
            try {{
                const packet = await window.route.get(replyPath + '|');
                const status = packet.getHeader('Request-Status');
                if (!status) return false;

                if (status === 'approved') {{
                    setStatus(joinStatus, 'Approved. Opening site...', 'ok');
                    window.address.href = 'hppr://' + GROUP + '/' + APP + '/';
                    return true;
                }}
                if (status === 'denied') {{
                    setStatus(joinStatus, 'Join request denied.', 'error');
                    return true;
                }}
                setStatus(joinStatus, 'Request status: ' + status, 'ok');
                return true;
            }} catch (_e) {{
                return false;
            }}
        }}

        async function requestJoin() {{
            if (!window.route) {{
                setStatus(joinStatus, 'window.route is unavailable for this page.', 'error');
                return;
            }}

            joinBtn.disabled = true;
            setStatus(joinStatus, 'Submitting join request...');

            if (JOIN_FIXTURE === 'pending') {{
                setStatus(joinStatus, 'Request status: pending (fixture)', 'ok');
                return;
            }}
            if (JOIN_FIXTURE === 'approved') {{
                setStatus(joinStatus, 'Approved. Opening site... (fixture)', 'ok');
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
                setStatus(joinStatus, 'Request sent. Waiting for response...');

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
                    setStatus(joinStatus, 'Watch failed. Refresh to retry.', 'error');
                }};
                watch.onclose = function() {{
                    if (!joinStatus || !joinStatus.classList.contains('ok')) {{
                        joinBtn.disabled = false;
                    }}
                }};
            }} catch (e) {{
                setStatus(joinStatus, 'Join request failed: ' + (e && e.message ? e.message : String(e)), 'error');
                joinBtn.disabled = false;
            }}
        }}

        copyBtn.addEventListener('click', async function() {{
            try {{
                await navigator.clipboard.writeText(ROUTE_VKEY);
                setStatus(joinStatus, 'Route key copied.', 'ok');
            }} catch (_e) {{
                setStatus(joinStatus, 'Failed to copy route key.', 'error');
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
