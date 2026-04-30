/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HAVI page handler.
//!
//! Handles `havi://` URLs for browser-owned internal pages.
//!
//! Surviving pages:
//! - `/home`: browser home page
//! - `/home-repo`: browser-owned local runtime and named-client management
//! - `/diagnostics`: explicit privileged diagnostics page

use std::collections::HashMap;
use std::sync::Arc;

use hppr_client::{Signer, ViaSpec, parse_via};

use crate::PageResponse;
use crate::hppr::client::HpprdClientAsync;
use crate::hppr::credentials::CredentialStoreHandle;
use crate::hppr::local_runtime::{default_repo_backed_runtime_is_local, global_local_runtime};
use crate::hppr::state_db::global_state_db;
use crate::hppr::util::html_escape;

/// Handle a `havi://` URL request.
pub async fn handle_request(
    url: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    let path = url.strip_prefix("havi:").unwrap_or(url);
    let path = path.trim_start_matches('/');
    let path = format!("/{}", path);

    if path.starts_with("/diagnostics/api") {
        let json = crate::pages::havi_diagnostics::handle_diagnostics_api(
            &path,
            client,
            credential_store,
        )
        .await;
        return PageResponse::new("application/json", json.into_bytes());
    }

    if path.starts_with("/home-repo/api") {
        let json = handle_home_repo_api(&path, client).await;
        return PageResponse::new("application/json", json.into_bytes());
    }

    let html = match path.as_str() {
        "/home" => render_home_page(),
        "/" | "" | "/diagnostics" => render_diagnostics_page(),
        "/home-repo" => render_home_repo_page(),
        _ => render_not_found(&path),
    };

    PageResponse::html(html)
}

/// Minimal helper-page styles with basic layout and legibility.
const ADMIN_CSS: &str = r#"
    body {
        margin: 0;
        padding: 1rem;
        background: #fff;
        color: #111;
        max-width: 980px;
        min-height: auto;
    }
    a { color: inherit; }
    h1, h2, h3 { color: inherit; }

    .nav ul {
        list-style: none;
        margin: 0 0 1rem 0;
        padding: 0;
        display: flex;
        flex-wrap: wrap;
        gap: 0.4rem;
    }
    .nav a {
        display: inline-block;
        padding: 0.2rem 0.45rem;
        border: 1px solid #ccc;
        text-decoration: none;
    }
    .nav a.active {
        font-weight: 600;
        background: #f5f5f5;
    }

    .card {
        border: 1px solid #ddd;
        padding: 0.85rem;
        margin-bottom: 0.85rem;
    }

    .status {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
        gap: 0.45rem;
    }
    .status-item {
        border: 1px solid #eee;
        padding: 0.45rem;
    }
    .status-value { font-family: monospace; font-size: 1.05rem; }
    .status-label { font-size: 0.9rem; color: #444; }

    button,
    input[type="text"] {
        font: inherit;
        padding: 0.25rem 0.5rem;
        border: 1px solid #bbb;
        background: #fff;
        color: inherit;
    }
    button + button { margin-left: 0.35rem; }

    .list-item {
        display: flex;
        justify-content: space-between;
        gap: 0.5rem;
        border-top: 1px solid #eee;
        padding: 0.45rem 0;
    }

    .message {
        margin-bottom: 0.85rem;
        border: 1px solid #bbb;
        padding: 0.5rem;
    }
    .message.error { border-color: #b33; background: #fff6f6; }
    .message.success { border-color: #3a7; background: #f6fff8; }

    .empty { color: #666; font-style: italic; }
    .muted { color: #555; font-size: 0.95rem; }
    .inline-row {
        display: flex;
        gap: 0.5rem;
        flex-wrap: wrap;
        align-items: center;
    }
    .codebox {
        border: 1px solid #ddd;
        background: #fafafa;
        padding: 0.55rem;
        word-break: break-all;
        white-space: pre-wrap;
    }

    .section-title {
        margin: 1rem 0 0.4rem;
        padding-bottom: 0.2rem;
        border-bottom: 1px solid #ddd;
        font-weight: 600;
    }
"#;

fn render_admin_page(title: &str, active_nav: &str, extra_css: &str, body_content: &str) -> String {
    let css = format!("{}{}", ADMIN_CSS, extra_css);
    let body = format!(
        "    <h1>{}</h1>\n    {}\n{}",
        title,
        render_nav(active_nav),
        body_content
    );
    crate::pages::page_shell::render_page(&format!("{} - HAVI", title), &css, &body)
}

fn render_home_page() -> String {
    let home_js = include_str!("../js/havi-home.js");
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>HAVI</title>
    <style>
        body {{
            font-family: system-ui, sans-serif;
            margin: 0;
            padding: 1rem;
            max-width: 760px;
        }}
        h1 {{ margin-bottom: 0.25rem; }}
        .muted {{ margin-top: 0; color: #444; }}
        #urlInput {{
            width: 100%;
            font: inherit;
            padding: 0.4rem 0.55rem;
            border: 1px solid #bbb;
            margin: 0.5rem 0 1rem;
        }}
        #quickLinks {{ display: flex; flex-wrap: wrap; gap: 0.4rem; }}
        #quickLinks a,
        .admin-link {{
            border: 1px solid #ccc;
            padding: 0.2rem 0.45rem;
            text-decoration: none;
            color: inherit;
        }}
    </style>
</head>
<body>
    <h1>HAVI</h1>
    <p class="muted">HPPR browser</p>

    <label for="urlInput">Address</label>
    <input
        type="text"
        id="urlInput"
        placeholder="hppr://... or //group/app/path"
        autofocus
    >

    <p>Quick links</p>
    <div id="quickLinks">
        <a href="hppr://u/" class="quick-link">//u/</a>
    </div>

    <p>
        <a href="havi:///home-repo" class="admin-link">Home repo</a>
        <a href="havi:///diagnostics" class="admin-link">Diagnostics</a>
    </p>

    <script>
{home_js}
    </script>
</body>
</html>"#,
        home_js = home_js
    )
}

fn render_nav(active: &str) -> String {
    let pages = [
        ("havi:///home-repo", "Home Repo"),
        ("havi:///diagnostics", "Diagnostics"),
    ];

    let links: Vec<String> = pages
        .iter()
        .map(|(href, label)| {
            let class = if *label == active {
                " class=\"active\""
            } else {
                ""
            };
            format!(r#"<li><a href="{}"{}>{}</a></li>"#, href, class, label)
        })
        .collect();

    format!(r#"<nav class="nav"><ul>{}</ul></nav>"#, links.join("\n"))
}

fn parse_havi_api_params(path: &str) -> HashMap<String, String> {
    let query = path.split('?').nth(1).unwrap_or("");
    url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect()
}

fn normalize_named_client_origin(origin_or_url: &str) -> Result<String, String> {
    let url = servo_url::BrowserUrl::parse(origin_or_url)
        .map_err(|error| format!("invalid origin or page URL '{}': {}", origin_or_url, error))?;
    let normalized = url.origin();
    if !normalized.is_tuple() {
        return Err(format!(
            "named-client grants require a tuple origin or full page URL, got '{}'",
            origin_or_url
        ));
    }
    Ok(normalized.ascii_serialization())
}

fn target_port(target: &ViaSpec) -> Option<u16> {
    match target {
        ViaSpec::Net { port, .. } => Some(*port),
        _ => None,
    }
}

async fn remote_runtime_status(client: &Arc<HpprdClientAsync>) -> Result<serde_json::Value, String> {
    let identity = client.get_packet_authenticated("//repo/admin/identity/|").await?;
    let repo_name = identity.header("Repo-Name").unwrap_or("localhost").to_string();
    let verifying_key = identity
        .header("Seal-By")
        .unwrap_or("")
        .to_string();
    let target = client.target();
    let port = target_port(&target);
    Ok(serde_json::json!({
        "mode": "remote",
        "repoName": repo_name,
        "verifyingKey": verifying_key,
        "repoTarget": target.to_string(),
        "repoPath": "(remote home repo)",
        "status": "remote-home",
        "backend": "hpprd",
        "version": "hpprd",
        "uptime": null,
        "port": port,
        "wsPort": port.map(|p| p.saturating_add(1)),
        "quibPort": port.map(|p| p.saturating_sub(1)),
        "udpPort": port,
    }))
}

async fn set_remote_repo_name(
    client: &Arc<HpprdClientAsync>,
    normalized: &str,
) -> Result<serde_json::Value, String> {
    let header_lines = [
        "Seal-By: ring0".to_string(),
        "Group: repo".to_string(),
        "App: admin".to_string(),
        "Location: identity".to_string(),
        format!("Repo-Name: {}", normalized),
    ];
    let mut add_payload = header_lines.join("\n");
    add_payload.push('\n');
    client.add(add_payload.as_bytes()).await?;
    Ok(serde_json::json!({"name": normalized}))
}

async fn handle_home_repo_api(path: &str, client: &Arc<HpprdClientAsync>) -> String {
    let params = parse_havi_api_params(path);
    let cmd = params.get("cmd").map(String::as_str).unwrap_or("named_clients");
    let db = global_state_db();

    let response = match cmd {
        "runtime_status" => {
            if default_repo_backed_runtime_is_local() {
                let runtime = global_local_runtime();
                serde_json::json!({
                    "ok": true,
                    "data": {
                        "mode": "local",
                        "repoName": runtime.repo_name(),
                        "verifyingKey": runtime.verifying_key(),
                        "packetStorePath": runtime.packet_store_path().display().to_string(),
                        "status": runtime.status_label(),
                        "backend": runtime.backend_label(),
                        "version": "browser-owned",
                        "uptime": null,
                        "port": null,
                        "wsPort": null,
                        "quibPort": null,
                        "udpPort": null,
                    }
                })
            } else {
                match remote_runtime_status(client).await {
                    Ok(data) => serde_json::json!({ "ok": true, "data": data }),
                    Err(error) => serde_json::json!({ "ok": false, "error": error }),
                }
            }
        }
        "set_repo_name" => {
            let Some(name) = params.get("name") else {
                return serde_json::json!({"ok": false, "error": "missing name"}).to_string();
            };
            let normalized = name.trim();
            if normalized.is_empty() {
                return serde_json::json!({"ok": false, "error": "empty name"}).to_string();
            }
            if default_repo_backed_runtime_is_local() {
                match db.set_setting("local_runtime_repo_name", normalized) {
                    Ok(()) => serde_json::json!({"ok": true, "data": {"name": normalized}}),
                    Err(error) => serde_json::json!({"ok": false, "error": error}),
                }
            } else {
                match set_remote_repo_name(client, normalized).await {
                    Ok(data) => serde_json::json!({ "ok": true, "data": data }),
                    Err(error) => serde_json::json!({ "ok": false, "error": error }),
                }
            }
        }
        "local_add" => {
            let Some(group) = params.get("group") else {
                return serde_json::json!({"ok": false, "error": "missing group"}).to_string();
            };
            let Some(app) = params.get("app") else {
                return serde_json::json!({"ok": false, "error": "missing app"}).to_string();
            };
            let Some(location) = params.get("location") else {
                return serde_json::json!({"ok": false, "error": "missing location"}).to_string();
            };
            if !default_repo_backed_runtime_is_local() {
                return serde_json::json!({"ok": false, "error": "local_add is only available in browser-local mode"}).to_string();
            }
            let content_type = params
                .get("content_type")
                .map(String::as_str)
                .unwrap_or("text/html; charset=utf-8");
            let body = params.get("data").cloned().unwrap_or_default();
            let runtime = global_local_runtime();
            match runtime.store_text_page(group.trim(), app.trim(), location.trim(), content_type, body.as_bytes()) {
                Ok(hashes) => serde_json::json!({"ok": true, "data": {"hashes": hashes}}),
                Err(error) => serde_json::json!({"ok": false, "error": error}),
            }
        }
        "named_clients" => {
            let clients = match db.list_named_clients() {
                Ok(clients) => clients,
                Err(error) => {
                    return serde_json::json!({"ok": false, "error": error}).to_string();
                },
            };
            let grants = match db.list_named_client_grants() {
                Ok(grants) => grants,
                Err(error) => {
                    return serde_json::json!({"ok": false, "error": error}).to_string();
                },
            };
            let revocations = match db.list_named_client_revocations() {
                Ok(revocations) => revocations,
                Err(error) => {
                    return serde_json::json!({"ok": false, "error": error}).to_string();
                },
            };
            serde_json::json!({
                "ok": true,
                "data": {
                    "clients": clients,
                    "grants": grants,
                    "revocations": revocations,
                }
            })
        }
        "set_named_client" => {
            let Some(name) = params.get("name") else {
                return serde_json::json!({"ok": false, "error": "missing name"}).to_string();
            };
            let Some(endpoint) = params.get("endpoint") else {
                return serde_json::json!({"ok": false, "error": "missing endpoint"}).to_string();
            };
            let Some(signer) = params.get("signer") else {
                return serde_json::json!({"ok": false, "error": "missing signer"}).to_string();
            };
            if name.trim().is_empty() {
                return serde_json::json!({"ok": false, "error": "empty name"}).to_string();
            }
            if let Err(error) = parse_via(endpoint) {
                return serde_json::json!({"ok": false, "error": format!("invalid endpoint: {}", error)}).to_string();
            }
            if let Err(error) = Signer::parse(signer) {
                return serde_json::json!({"ok": false, "error": format!("invalid signer: {}", error)}).to_string();
            }
            match db.set_named_client(name.trim(), endpoint.trim(), signer.trim()) {
                Ok(()) => serde_json::json!({"ok": true, "data": {"name": name.trim()}}),
                Err(error) => serde_json::json!({"ok": false, "error": error}),
            }
        }
        "delete_named_client" => {
            let Some(name) = params.get("name") else {
                return serde_json::json!({"ok": false, "error": "missing name"}).to_string();
            };
            match db.delete_named_client(name.trim()) {
                Ok(()) => serde_json::json!({"ok": true, "data": {"name": name.trim()}}),
                Err(error) => serde_json::json!({"ok": false, "error": error}),
            }
        }
        "grant_named_client" => {
            let Some(name) = params.get("name") else {
                return serde_json::json!({"ok": false, "error": "missing name"}).to_string();
            };
            let Some(origin) = params.get("origin") else {
                return serde_json::json!({"ok": false, "error": "missing origin"}).to_string();
            };
            let normalized_origin = match normalize_named_client_origin(origin.trim()) {
                Ok(origin) => origin,
                Err(error) => {
                    return serde_json::json!({"ok": false, "error": error}).to_string();
                }
            };
            match db.grant_named_client(&normalized_origin, name.trim()) {
                Ok(()) => serde_json::json!({
                    "ok": true,
                    "data": {"name": name.trim(), "origin": normalized_origin}
                }),
                Err(error) => serde_json::json!({"ok": false, "error": error}),
            }
        }
        "revoke_named_client" => {
            let Some(name) = params.get("name") else {
                return serde_json::json!({"ok": false, "error": "missing name"}).to_string();
            };
            let Some(origin) = params.get("origin") else {
                return serde_json::json!({"ok": false, "error": "missing origin"}).to_string();
            };
            let normalized_origin = match normalize_named_client_origin(origin.trim()) {
                Ok(origin) => origin,
                Err(error) => {
                    return serde_json::json!({"ok": false, "error": error}).to_string();
                }
            };
            match db.revoke_named_client(&normalized_origin, name.trim()) {
                Ok(()) => serde_json::json!({
                    "ok": true,
                    "data": {"name": name.trim(), "origin": normalized_origin}
                }),
                Err(error) => serde_json::json!({"ok": false, "error": error}),
            }
        }
        _ => serde_json::json!({"ok": false, "error": format!("unknown command: {}", cmd)}),
    };

    response.to_string()
}

fn render_home_repo_page() -> String {
    let home_repo_js = include_str!("../js/havi-home-repo.js");
    let body = format!(
        r#"
    <div class="card">
        <h2>Home Repo Status</h2>
        <div class="status">
            <div class="status-item">
                <div class="status-value" id="port">...</div>
                <div class="status-label">Port</div>
            </div>
            <div class="status-item">
                <div class="status-value" id="wsPort">...</div>
                <div class="status-label">WebSocket</div>
            </div>
            <div class="status-item">
                <div class="status-value" id="quibPort">...</div>
                <div class="status-label">QUIB</div>
            </div>
            <div class="status-item">
                <div class="status-value" id="udpPort">...</div>
                <div class="status-label">UDP</div>
            </div>
            <div class="status-item">
                <div class="status-value" id="status">...</div>
                <div class="status-label">Mode</div>
            </div>
        </div>
        <p class="muted">
            Repo: <code id="repoPath">...</code>
        </p>
    </div>

    <div class="card">
        <h2>Home Repo Verification Key</h2>
        <p class="muted">This key identifies your home repo to route repos and peers.</p>
        <div id="repoKey" class="codebox">Loading...</div>
    </div>

    <div class="card" id="daemonInfoCard" style="display: none;">
        <h2>Runtime Info</h2>
        <div class="status">
            <div class="status-item">
                <div class="status-value" id="daemonStatus">...</div>
                <div class="status-label">Status</div>
            </div>
            <div class="status-item">
                <div class="status-value" id="daemonUptime">...</div>
                <div class="status-label">Uptime</div>
            </div>
            <div class="status-item">
                <div class="status-value" id="daemonBackend">...</div>
                <div class="status-label">Backend</div>
            </div>
        </div>
        <p class="muted">
            Version: <code id="daemonVersion">...</code>
        </p>
    </div>

    <div class="card">
        <h2>Repo Name</h2>
        <p class="muted">This name identifies your repo in HELLO greetings.</p>
        <div class="inline-row">
            <input type="text" id="repoName" placeholder="Repo name">
            <button onclick="saveRepoName()" id="saveNameBtn">Save</button>
        </div>
        <div id="nameMessage" class="muted"></div>
    </div>

    <div class="card">
        <h2>Named Clients</h2>
        <p class="muted">Explicit extra repo profiles with origin-scoped grants.</p>
        <div class="inline-row">
            <input type="text" id="namedClientName" placeholder="Client name">
            <input type="text" id="namedClientEndpoint" placeholder="Endpoint, e.g. tcp+127.0.0.1:4778">
            <input type="text" id="namedClientSigner" placeholder="Signer, e.g. ring1:ring0|init">
            <button onclick="saveNamedClient()">Save Client</button>
        </div>
        <div id="namedClientMessage" class="muted"></div>
        <div id="namedClientsList"><p class="empty">Loading...</p></div>
        <div class="section-title">Revocation Log</div>
        <div id="namedClientRevocations"><p class="empty">None</p></div>
    </div>

    <script>
{home_repo_js}
    </script>"#,
        home_repo_js = home_repo_js
    );

    render_admin_page("Home Repo Configuration", "Home Repo", "", &body)
}

fn render_diagnostics_page() -> String {
    let diagnostics_js = include_str!("../js/havi-diagnostics.js");
    let body = format!(
        r#"
    <div id="message"></div>

    <div class="card">
        <h2>Inspect Route/Deploy/Auth</h2>
        <div class="inline-row">
            <input type="text" id="diagGroup" placeholder="group" value="u">
            <input type="text" id="diagApp" placeholder="app" value="web">
            <input type="text" id="diagLocation" placeholder="location (optional)">
            <button onclick="runDiagnostics()">Inspect</button>
        </div>
        <p class="muted">Reads local route, remote deploy pointer, and auth probe state.</p>
        <pre id="diagOutput" class="codebox">Click Inspect</pre>
    </div>

    <script>
{diagnostics_js}
    </script>"#,
        diagnostics_js = diagnostics_js
    );

    render_admin_page("Diagnostics", "Diagnostics", "", &body)
}

fn render_not_found(path: &str) -> String {
    let body = format!(
        r#"
    <div class="card">
        <p class="error">The requested page was not found: {path}</p>
        <p><a href="havi:///home-repo">Open home repo</a></p>
        <p><a href="havi:///diagnostics">Open diagnostics</a></p>
    </div>"#,
        path = html_escape(path),
    );
    render_admin_page("Page Not Found", "", "", &body)
}
