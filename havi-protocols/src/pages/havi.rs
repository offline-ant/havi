/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HAVI page handler.
//!
//! Handles havi:// URLs for browser internal pages.
//! URL format: havi:///[page]
//!
//! Pages:
//! - /overview: Navigation hub to all sections
//! - /home: Browser home page
//! - /home-repo: Home repo configuration and status
//! - /routes: Trusted routes and keys
//! - /anyone: Edit anyone account ACL
//! - /ring2: Group membership management (stub)
//! - /ring1: View account requests (stub)
//! - /ring0: Ring0 proxy page for ring1 proxy requests
//! - /diagnostics: Route/deploy/auth/join diagnostics + join fixtures
//! - /services: Pylon service manager (services, listeners, mounts, nat)

use std::sync::Arc;

use crate::PageResponse;
use crate::client::{HpprdClientAsync, get_admin_credentials};
use crate::credentials::CredentialStoreHandle;
use crate::state_db::global_state_db;
use crate::util::html_escape;

/// Handle an havi:// URL request.
///
/// Returns a PageResponse with admin credentials set for window.ring0 access.
pub async fn handle_request(
    url: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    let path = url.strip_prefix("havi:").unwrap_or(url);
    let path = path.trim_start_matches('/');
    let path = format!("/{}", path);

    // Handle services API endpoint
    if path.starts_with("/services/api") {
        let json = handle_services_api(&path);
        return PageResponse::new("application/json", json.into_bytes());
    }

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

    let html = match path.as_str() {
        "/home" => render_home_page(),
        "/overview" | "/" | "" => render_dashboard(),
        "/home-repo" => render_home_repo_page(),
        "/routes" => render_routes_page(),
        "/anyone" => render_anyone_page(),
        "/ring2" => render_groups_page(),
        "/ring1" => render_accounts_page(),
        "/ring0" => render_ring0_proxy_page(),
        "/diagnostics" => render_diagnostics_page(),
        "/services" => render_services_page(),
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
    crate::page_shell::render_page(&format!("{} - HAVI", title), &css, &body)
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

    <p><a href="havi:///overview" class="admin-link">Open admin pages</a></p>

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
        ("havi:///overview", "Overview"),
        ("havi:///home-repo", "Home Repo"),
        ("havi:///routes", "Routes"),
        ("havi:///anyone", "Anyone"),
        ("havi:///ring2", "Ring2"),
        ("havi:///ring1", "Ring1"),
        ("havi:///ring0", "Ring0"),
        ("havi:///diagnostics", "Diagnostics"),
        ("havi:///services", "Services"),
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

/// Render the dashboard page.
fn render_dashboard() -> String {
    let history_rows = global_state_db().list_history(100).unwrap_or_default();

    let history_html = if history_rows.is_empty() {
        "<p class=\"empty\">No history yet.</p>".to_string()
    } else {
        let items: Vec<String> = history_rows
            .iter()
            .map(|row| {
                let url = html_escape(&row.url);
                let title = if row.title.trim().is_empty() {
                    "(untitled)".to_string()
                } else {
                    html_escape(&row.title)
                };
                format!(
                    r#"<div class="list-item">
                        <div>
                            <div><a href="{url}">{title}</a></div>
                            <div class="muted">{url}</div>
                        </div>
                        <div class="muted">{ts}</div>
                    </div>"#,
                    url = url,
                    title = title,
                    ts = row.ts_unix,
                )
            })
            .collect();
        items.join("\n")
    };

    let body = format!(
        r#"
    {nav}

    <div class="card">
        <div class="muted">recent pages</div>
        {history_html}
    </div>

    "#,
        nav = render_nav("Overview"),
        history_html = history_html,
    );

    crate::page_shell::render_page("HAVI", ADMIN_CSS, &body)
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
        <h2>Account Management</h2>
        <p class="muted">Manage ring1 accounts and their ACL rules. System accounts cannot be deleted.</p>

        <div class="section-title">System Accounts</div>
        <div id="systemAccounts">Loading...</div>

        <div class="section-title">Site Sandboxes (HAVI-managed)</div>
        <div id="sandboxAccounts"><p class="empty">None</p></div>

        <div class="section-title">Custom Accounts</div>
        <div id="customAccounts"><p class="empty">None</p></div>

        <div class="section-title">Pending Requests</div>
        <div id="pendingRequests"><p class="empty">None</p></div>

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
            Group setup: <code>//<em>group</em>/admin/setup/|</code><br>
            Membership: <code>//<em>group</em>/admin/members/|/seal/&lt;key&gt;</code>
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

/// Render route/deploy/auth/join diagnostics page.
fn render_diagnostics_page() -> String {
    let diagnostics_js = include_str!("../js/havi-diagnostics.js");
    let body = format!(
        r#"
    <div id="message"></div>

    <div class="card">
        <h2>Inspect Route/Deploy/Auth/Join</h2>
        <div class="inline-row">
            <input type="text" id="diagGroup" placeholder="group" value="u">
            <input type="text" id="diagApp" placeholder="app" value="web">
            <input type="text" id="diagLocation" placeholder="location (optional)">
            <button onclick="runDiagnostics()">Inspect</button>
        </div>
        <p class="muted">Reads local route, remote deploy, auth probe, and join request/reply status.</p>
        <pre id="diagOutput" class="codebox">Click Inspect</pre>
    </div>

    <div class="card">
        <h2>Join Fixture State</h2>
        <p class="muted">Process-local deterministic join state for hppr-join testing.</p>
        <div class="inline-row">
            <button onclick="setJoinFixture('none')">none</button>
            <button onclick="setJoinFixture('pending')">pending</button>
            <button onclick="setJoinFixture('approved')">approved</button>
        </div>
        <p class="muted">Current: <span id="joinFixtureState">loading...</span></p>
    </div>

    <script>
{diagnostics_js}
    </script>"#,
        diagnostics_js = diagnostics_js
    );

    render_admin_page("Diagnostics", "Diagnostics", "", &body)
}

/// Render the services page.
fn render_services_page() -> String {
    let services_js = include_str!("../js/havi-services.js");
    let extra_css = "";

    let body = format!(
        r#"
    <div id="message"></div>

    <div class="card">
        <h2>Pylon Status</h2>
        <div class="status">
            <div class="status-item">
                <div class="status-value" id="pylonStatus">checking...</div>
                <div class="status-label">Connection</div>
            </div>
        </div>
        <p class="muted">Manage hpprd, hppr-nat, lokid, unlokid, hppr-nfs, hppr-fuse.</p>
        <div id="servicesList"><p class="empty">Loading...</p></div>
    </div>

    <div class="card">
        <h2>hpprd Listeners</h2>
        <div id="listenersList"><p class="empty">Loading...</p></div>
        <div class="inline-row">
            <input type="text" id="listenerBind" placeholder="ws+127.0.0.1:4778">
            <button onclick="addListener()">Add Listener</button>
        </div>
    </div>

    <div class="card">
        <h2>NAT Runtime</h2>
        <div id="natInfo"><p class="empty">Loading...</p></div>
    </div>

    <div class="card">
        <h2>Mounts</h2>
        <div id="mountsList"><p class="empty">Loading...</p></div>
        <div class="inline-row">
            <input type="text" id="mountpoint" placeholder="/mnt/hppr">
            <input type="text" id="mountRoot" placeholder="// (optional root)">
            <input type="text" id="mountSigner" placeholder="(optional signer)">
            <label><input type="checkbox" id="mountRw"> rw</label>
            <button onclick="createMount()">Mount</button>
        </div>
    </div>

    <p><button onclick="loadStatus()" class="secondary">Refresh</button></p>

    <script>
{services_js}
    </script>"#,
        services_js = services_js
    );

    render_admin_page("Services", "Services", extra_css, &body)
}

/// Handle services API requests (proxied to pylon).
fn handle_services_api(path: &str) -> String {
    let query = path.split('?').nth(1).unwrap_or("");
    let params: std::collections::HashMap<String, String> =
        url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();

    let cmd = params.get("cmd").map(|s| s.as_str()).unwrap_or("status");
    let service = params.get("service").map(|s| s.as_str());

    let mut args = serde_json::Map::new();
    for (k, v) in &params {
        if k == "cmd" || k == "service" || v.is_empty() {
            continue;
        }
        let value = if v.eq_ignore_ascii_case("true") {
            serde_json::Value::Bool(true)
        } else if v.eq_ignore_ascii_case("false") {
            serde_json::Value::Bool(false)
        } else if let Ok(n) = v.parse::<i64>() {
            serde_json::json!(n)
        } else {
            serde_json::Value::String(v.clone())
        };
        args.insert(k.clone(), value);
    }

    let mut client = match crate::pylon::PylonClient::try_connect(&crate::config::repo_dir()) {
        Some(c) => c,
        None => {
            return serde_json::json!({"ok": false, "error": "Pylon is not running. Start pylon first."})
                .to_string();
        },
    };

    let result = match cmd {
        "status" | "list" | "mounts" | "shutdown" => {
            client.command(cmd, None, if args.is_empty() { None } else { Some(&args) })
        },
        "start" | "stop" => {
            let Some(name) = service else {
                return serde_json::json!({"ok": false, "error": "missing service parameter"})
                    .to_string();
            };
            client.command(
                cmd,
                Some(name),
                if args.is_empty() { None } else { Some(&args) },
            )
        },
        "listen" | "unlisten" | "mount" | "unmount" => {
            client.command(cmd, None, if args.is_empty() { None } else { Some(&args) })
        },
        _ => {
            return serde_json::json!({"ok": false, "error": format!("unknown command: {}", cmd)})
                .to_string();
        },
    };

    match result {
        Ok(data) => serde_json::json!({"ok": true, "data": data}).to_string(),
        Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
    }
}

/// Render not found page.
fn render_not_found(path: &str) -> String {
    let body = format!(
        r#"
    <div class="card">
        <p class="error">The requested page was not found: {path}</p>
        <p><a href="havi:///overview">Return to Overview</a></p>
    </div>"#,
        path = html_escape(path),
    );
    render_admin_page("Page Not Found", "", "", &body)
}
