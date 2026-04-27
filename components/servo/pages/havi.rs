/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HAVI page handler.
//!
//! Handles havi:// URLs for browser internal pages.
//! URL format: havi:///[page]
//!
//! Pages:
//! - /home: Browser home page
//! - /home-repo: Home repo configuration and status
//! - /routes: Trusted routes and keys
//! - /anyone: Edit anyone account ACL
//! - /ring2: Group membership management (stub)
//! - /ring1: Ring1 auth/policy management
//! - /ring0: Ring0 proxy page for ring1 proxy requests
//! - /diagnostics: privileged diagnostics page

use std::collections::HashMap;
use std::sync::Arc;

use hppr_client::{Signer, parse_via};

use crate::PageResponse;
use crate::hppr::client::{HpprdClientAsync, get_admin_credentials};
use crate::hppr::credentials::CredentialStoreHandle;
use crate::hppr::state_db::global_state_db;
use crate::hppr::util::html_escape;

/// Handle an havi:// URL request.
///
/// Returns a PageResponse with admin credentials set for
/// `window.havi.admin.client` access.
pub async fn handle_request(
    url: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    let path = url.strip_prefix("havi:").unwrap_or(url);
    let path = path.trim_start_matches('/');
    let path = format!("/{}", path);

    // Handle diagnostics API endpoint
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
        let json = handle_home_repo_api(&path);
        return PageResponse::new("application/json", json.into_bytes());
    }

    let html = match path.as_str() {
        "/home" => render_home_page(),
        "/" | "" | "/diagnostics" => render_diagnostics_page(),
        "/home-repo" => render_home_repo_page(),
        "/routes" => render_routes_page(),
        "/anyone" => render_anyone_page(),
        "/ring2" => render_groups_page(),
        "/ring1" => render_accounts_page(),
        "/ring0" => render_ring0_proxy_page(),
        _ => render_not_found(&path),
    };

    let (ring1_name, token) = get_admin_credentials();
    PageResponse::html(html).with_admin_credentials(ring1_name, token)
}

/// Minimal admin-page styles with basic layout and legibility.
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
    .list-item .name,
    .route-title,
    .route-value,
    .request-ring1,
    .request-cmd,
    .request-detail,
    .account-name,
    .account-rules,
    .acl-coord {
        font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
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

    .account-item,
    .route-item,
    .request-card {
        border-top: 1px solid #eee;
        padding: 0.55rem 0;
    }
    .account-header,
    .route-header,
    .request-header {
        display: flex;
        justify-content: space-between;
        gap: 0.6rem;
        flex-wrap: wrap;
        align-items: center;
    }
    .account-rules { margin-top: 0.35rem; font-size: 0.92rem; }

    .acl-editor { border: 1px solid #ddd; margin-top: 0.4rem; }
    .acl-header,
    .acl-row,
    .add-row {
        display: grid;
        grid-template-columns: 1fr 2.3rem 2.3rem 2.3rem 2.3rem;
        gap: 0.25rem;
        align-items: center;
        padding: 0.35rem;
        border-top: 1px solid #eee;
    }
    .acl-header { border-top: 0; font-weight: 600; }
    .perm-btn,
    .remove-btn,
    .add-btn {
        padding: 0.2rem;
        min-width: 2rem;
    }
    .add-row input { width: 100%; }
    .editor-buttons,
    .save-section,
    .btn-group { display: flex; gap: 0.4rem; flex-wrap: wrap; }
    .inline-editor { margin-top: 0.55rem; }

    .acl-tree details { margin-left: 0.8rem; }
    .acl-tree summary { cursor: pointer; }
"#;

/// Render an admin page with shared shell, admin CSS, and nav bar.
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

/// Render the home page (new tab page).
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

    <p><a href="havi:///diagnostics" class="admin-link">Open diagnostics</a></p>

    <script>
{home_js}
    </script>
</body>
</html>"#,
        home_js = home_js
    )
}

/// Render navigation bar.
fn render_nav(active: &str) -> String {
    let pages = [
        ("havi:///home-repo", "Home Repo"),
        ("havi:///routes", "Routes"),
        ("havi:///anyone", "Anyone"),
        ("havi:///ring2", "Ring2"),
        ("havi:///ring1", "Ring1"),
        ("havi:///ring0", "Ring0"),
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

fn handle_home_repo_api(path: &str) -> String {
    let params = parse_havi_api_params(path);
    let cmd = params.get("cmd").map(String::as_str).unwrap_or("named_clients");
    let db = global_state_db();

    let response = match cmd {
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

/// Render the home repo configuration page.
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
        <h2>Daemon Info</h2>
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

/// Render the routes and trust page.
fn render_routes_page() -> String {
    let routes_js = include_str!("../js/havi-routes.js");
    let body = format!(
        r#"
    <div id="message"></div>

    <div class="card">
        <h2>Configured Routes</h2>
        <p class="muted">Routes tell HAVI where to fetch content for each group#app coordinate.</p>
        <div id="routesList">Loading...</div>
        <p><button onclick="loadRoutes()" class="secondary">Refresh</button></p>
    </div>

    <script>
{routes_js}
    </script>"#,
        routes_js = routes_js
    );

    render_admin_page("Routes &amp; Trust", "Routes", "", &body)
}

/// Render the ring1 accounts management page.
fn render_accounts_page() -> String {
    let ring1_js = include_str!("../js/havi-ring1.js");
    let extra_css = "";

    let body = format!(
        r#"
    <div id="message"></div>

    <div class="card">
        <h2>Ring1 Auth and Policy</h2>
        <p class="muted">Manage split Ring1 auth, members, and policy packet families. System accounts cannot be deleted.</p>

        <div class="section-title">System Accounts</div>
        <div id="systemAccounts">Loading...</div>

        <div class="section-title">Site Sandboxes (HAVI-managed)</div>
        <div id="sandboxAccounts"><p class="empty">None</p></div>

        <div class="section-title">Custom Accounts</div>
        <div id="customAccounts"><p class="empty">None</p></div>

        <p><button onclick="loadAll()" class="secondary">Refresh</button></p>
    </div>

    <script>
{ring1_js}
    </script>"#,
        ring1_js = ring1_js
    );

    render_admin_page("Ring1 Accounts", "Ring1", extra_css, &body)
}

/// Render the groups page (stub).
fn render_groups_page() -> String {
    render_admin_page(
        "Group Membership",
        "Ring2",
        "",
        r#"
    <div class="card">
        <h2>Group Management</h2>
        <p class="empty">Group membership management coming soon.</p>
        <p class="muted">
            Auth profile: <code>//<em>group</em>/admin/ring2/auth/|/seal/&lt;repo-vkey&gt;</code><br>
            Membership: <code>//<em>group</em>/admin/members/|/seal/&lt;key&gt;</code><br>
            Policy: <code>//<em>group</em>/admin/ring2/policy/|/seal/&lt;repo-vkey&gt;</code>
        </p>
    </div>"#,
    )
}

/// Render the Anyone account ACL editor page.
fn render_anyone_page() -> String {
    let anyone_js = include_str!("../js/havi-anyone.js");
    let extra_css = "";

    let body = format!(
        r#"
    <div id="message"></div>

    <div class="card">
        <h2>Access Rules</h2>
        <p class="muted">
            These rules control what unauthenticated requests can access.
            Click permission buttons to cycle: grant → deny → inherit.
        </p>

        <div class="acl-editor">
            <div class="acl-header">
                <span>Coordinate</span>
                <span title="Read">R</span>
                <span title="Write">W</span>
                <span title="List">L</span>
                <span></span>
            </div>
            <div id="rulesList"></div>
            <div class="add-row">
                <input type="text" id="newCoord" placeholder="//group/app/path">
                <button class="perm-btn perm-grant" id="newR" onclick="toggleNewPerm(0)">r</button>
                <button class="perm-btn perm-inherit" id="newW" onclick="toggleNewPerm(1)">.</button>
                <button class="perm-btn perm-inherit" id="newL" onclick="toggleNewPerm(2)">.</button>
                <button class="add-btn" onclick="addRule()" title="Add rule">+</button>
            </div>
        </div>

        <div class="save-section">
            <button onclick="saveRules()" id="saveBtn">Save Changes</button>
            <button onclick="loadRules()" class="secondary">Reset</button>
            <span id="dirtyIndicator" class="dirty-indicator" style="display: none;">
                Unsaved changes
            </span>
        </div>
    </div>

    <script>
{anyone_js}
    </script>"#,
        anyone_js = anyone_js
    );

    render_admin_page("Anyone Account", "Anyone", extra_css, &body)
}

/// Render the ring0 proxy page.
fn render_ring0_proxy_page() -> String {
    let ring0_js = include_str!("../js/havi-ring0.js");
    let extra_css = r#"
        .status-indicator {
            display: inline-block;
            width: 0.55rem;
            height: 0.55rem;
            border-radius: 50%;
            margin-right: 0.45rem;
            background: #888;
        }
        .status-watching { background: #2f7; }
        .status-error { background: #c33; }
    "#;

    let body = format!(
        r#"
    <div id="message"></div>

    <div class="card">
        <h2><span id="watchIndicator" class="status-indicator status-watching"></span>Pending Proxy Requests</h2>
        <p class="muted">
            Ring1 accounts submit proxy requests here. Approve to execute the command via ring0 and return the result.
        </p>
        <div id="requestsList"><p class="empty">Scanning...</p></div>
        <p><button onclick="scanRequests()" class="secondary">Refresh</button></p>
    </div>

    <script>
{ring0_js}
    </script>"#,
        ring0_js = ring0_js
    );

    render_admin_page("Ring0 Proxy", "Ring0", extra_css, &body)
}

/// Render privileged diagnostics page.
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


/// Render not found page.
fn render_not_found(path: &str) -> String {
    let body = format!(
        r#"
    <div class="card">
        <p class="error">The requested page was not found: {path}</p>
        <p><a href="havi:///diagnostics">Open diagnostics</a></p>
    </div>"#,
        path = html_escape(path),
    );
    render_admin_page("Page Not Found", "", "", &body)
}
