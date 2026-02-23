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
//! - /services: Pylon service manager (start/stop hpprd, lokid, unlokid, hppr-fs)

use crate::PageResponse;
use crate::client::get_admin_credentials;
use crate::util::html_escape;

/// Handle an havi:// URL request.
///
/// Returns a PageResponse with admin credentials set for window.ring0 access.
pub async fn handle_request(url: &str) -> PageResponse {
    let path = url.strip_prefix("havi:").unwrap_or(url);
    let path = path.trim_start_matches('/');
    let path = format!("/{}", path);

    // Handle services API endpoint
    if path.starts_with("/services/api") {
        let json = handle_services_api(&path);
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
        "/services" => render_services_page(),
        _ => render_not_found(&path),
    };

    let (ring1_name, token) = get_admin_credentials();
    PageResponse::html(html).with_admin_credentials(ring1_name, token)
}

/// Admin-page-specific styles (nav, cards, buttons, etc).
const ADMIN_CSS: &str = r#"
    h1 { margin: 0 0 20px 0; }
    h2 { color: #7fdbff; margin: 20px 0 10px 0; font-size: 1.2em; }
    .nav {
        display: flex;
        gap: 20px;
        margin-bottom: 20px;
        padding-bottom: 15px;
        border-bottom: 1px solid #333;
        flex-wrap: wrap;
    }
    .nav a {
        padding: 8px 16px;
        background: #16213e;
        border-radius: 6px;
    }
    .nav a:hover { background: #1f2d4a; text-decoration: none; }
    .nav a.active { background: #4ecdc4; color: #1a1a2e; }
    .card {
        background: #16213e;
        border-radius: 8px;
        padding: 20px;
        margin-bottom: 15px;
    }
    .status { display: flex; gap: 30px; }
    .status-item { text-align: center; }
    .status-value { font-size: 1.5em; color: #4ecdc4; font-family: monospace; }
    .status-label { color: #888; font-size: 0.9em; }
    button {
        padding: 10px 20px;
        font-size: 1em;
        border: none;
        border-radius: 6px;
        cursor: pointer;
        background: #27ae60;
        color: white;
    }
    button:hover { background: #2ecc71; }
    button:disabled { background: #555; cursor: not-allowed; }
    button.danger { background: #c0392b; }
    button.danger:hover { background: #e74c3c; }
    button.secondary { background: #555; }
    button.secondary:hover { background: #666; }
    input[type="text"] {
        padding: 10px;
        font-size: 1em;
        border: 2px solid #333;
        border-radius: 6px;
        background: #16213e;
        color: #eee;
        width: 200px;
    }
    input[type="text"]:focus { outline: none; border-color: #4ecdc4; }
    .list-item {
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding: 12px;
        background: #0f1729;
        border-radius: 6px;
        margin-bottom: 8px;
    }
    .list-item .name { font-family: monospace; }
    .empty { color: #666; font-style: italic; }
    .error { color: #ff6b6b; }
    .success { color: #27ae60; }
    .message {
        padding: 10px;
        border-radius: 6px;
        margin-bottom: 15px;
    }
    .message.error { background: #2d1f1f; border-left: 4px solid #ff6b6b; }
    .message.success { background: #1f2d1f; border-left: 4px solid #27ae60; }
    .route-item {
        background: #0f1729;
        border-radius: 6px;
        padding: 15px;
        margin-bottom: 10px;
    }
    .route-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 10px;
    }
    .route-title { font-size: 1.1em; font-family: monospace; color: #4ecdc4; }
    .route-detail {
        display: flex;
        gap: 10px;
        padding: 5px 0;
        font-size: 0.9em;
    }
    .route-label { color: #888; min-width: 100px; }
    .route-value { font-family: monospace; color: #7fdbff; word-break: break-all; }
    .trusted-key { color: #27ae60; }
    .key-truncated { font-size: 0.85em; }
"#;

/// Render an admin page with shared shell, admin CSS, and nav bar.
fn render_admin_page(title: &str, active_nav: &str, extra_css: &str, body_content: &str) -> String {
    let css = format!("{}{}", ADMIN_CSS, extra_css);
    let body = format!("    <h1>{}</h1>\n    {}\n{}", title, render_nav(active_nav), body_content);
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
        * {{ box-sizing: border-box; }}
        body {{
            font-family: system-ui, sans-serif;
            margin: 0;
            padding: 0;
            background: #1a1a2e;
            color: #eee;
            min-height: 100vh;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
        }}
        .container {{
            text-align: center;
            max-width: 600px;
            padding: 40px;
        }}
        .logo {{
            font-size: 4em;
            font-weight: bold;
            color: #4ecdc4;
            margin-bottom: 10px;
            letter-spacing: 0.1em;
        }}
        .tagline {{
            color: #7fdbff;
            font-size: 1.2em;
            margin-bottom: 40px;
        }}
        .search-box {{
            width: 100%;
            max-width: 500px;
            padding: 15px 20px;
            font-size: 1.1em;
            border: 2px solid #333;
            border-radius: 30px;
            background: #16213e;
            color: #eee;
            outline: none;
            transition: border-color 0.2s;
        }}
        .search-box:focus {{
            border-color: #4ecdc4;
        }}
        .search-box::placeholder {{
            color: #666;
        }}
        .quick-links {{
            display: flex;
            gap: 15px;
            margin-top: 40px;
            flex-wrap: wrap;
            justify-content: center;
        }}
        .quick-link {{
            padding: 12px 24px;
            background: #16213e;
            border-radius: 8px;
            color: #7fdbff;
            text-decoration: none;
            transition: background 0.2s;
        }}
        .quick-link:hover {{
            background: #1f2d4a;
            text-decoration: none;
        }}
        .admin-link {{
            position: fixed;
            bottom: 20px;
            right: 20px;
            padding: 10px 20px;
            background: #16213e;
            border-radius: 6px;
            color: #888;
            text-decoration: none;
            font-size: 0.9em;
        }}
        .admin-link:hover {{
            color: #4ecdc4;
            text-decoration: none;
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="logo">HAVI</div>
        <div class="tagline">HPPR Browser</div>

        <input type="text" class="search-box" id="urlInput"
               placeholder="Enter hppr:// address or //group/app/path"
               autofocus>

        <div class="quick-links" id="quickLinks">
            <a href="hppr://u/" class="quick-link">Browse //u/</a>
        </div>
    </div>

    <a href="havi:///overview" class="admin-link">Admin Settings</a>

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
        ("havi:///services", "Services"),
    ];

    let links: Vec<String> = pages
        .iter()
        .map(|(href, label)| {
            let class = if *label == active { " class=\"active\"" } else { "" };
            format!(r#"<a href="{}"{}>{}</a>"#, href, class, label)
        })
        .collect();

    format!(r#"<nav class="nav">{}</nav>"#, links.join("\n"))
}

/// Render the dashboard page.
fn render_dashboard() -> String {
    render_admin_page("HAVI", "Overview", "", r#"
    <div class="card">
        <h2>Browser Administration</h2>
        <p><a href="havi:///home-repo">Home Repo</a> - Port, repo path, and home repo status</p>
        <p><a href="havi:///routes">Routes &amp; Trust</a> - Manage route repo endpoints and keys</p>
        <p><a href="havi:///anyone">Anyone</a> - Edit anyone account permissions</p>
        <p><a href="havi:///ring2">Ring2</a> - Manage group membership</p>
        <p><a href="havi:///ring1">Ring1</a> - Manage ring1 accounts and requests</p>
        <p><a href="havi:///ring0">Ring0 Proxy</a> - Review and approve ring1 proxy requests</p>
        <p><a href="havi:///services">Services</a> - Pylon service manager (hpprd, lokid, unlokid, hppr-fs)</p>
    </div>"#)
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
        <p style="margin-top: 15px; color: #888; font-size: 0.9em;">
            Repo: <code id="repoPath" style="color: #7fdbff;">...</code>
        </p>
    </div>

    <div class="card">
        <h2>Home Repo Verification Key</h2>
        <p style="color: #888; font-size: 0.9em; margin-top: 0;">
            This key identifies your home repo to route repos and peers.
        </p>
        <div id="repoKey" style="font-family: monospace; background: #0f1729; padding: 15px; border-radius: 6px; color: #4ecdc4; word-break: break-all;">
            Loading...
        </div>
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
        <p style="margin-top: 15px; color: #888; font-size: 0.9em;">
            Version: <code id="daemonVersion" style="color: #7fdbff;">...</code>
        </p>
    </div>

    <div class="card">
        <h2>Repo Name</h2>
        <p style="color: #888; font-size: 0.9em; margin-top: 0;">
            This name identifies your repo in HELLO greetings.
        </p>
        <div style="display: flex; gap: 10px; align-items: center; margin-top: 15px;">
            <input type="text" id="repoName" placeholder="Repo name" style="width: 200px;">
            <button onclick="saveRepoName()" id="saveNameBtn">Save</button>
        </div>
        <div id="nameMessage" style="margin-top: 10px;"></div>
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
        <p style="color: #888; font-size: 0.9em; margin-top: 0;">
            Routes tell HAVI where to fetch content for each group#app coordinate.
        </p>
        <div id="routesList">Loading...</div>
        <button onclick="loadRoutes()" style="margin-top: 15px;" class="secondary">Refresh</button>
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
    let extra_css = r#"
        .section-title {
            color: #4ecdc4;
            font-size: 1.1em;
            margin: 20px 0 10px 0;
            padding-bottom: 5px;
            border-bottom: 1px solid #333;
        }
        .account-item {
            background: #0f1729;
            border-radius: 6px;
            padding: 15px;
            margin-bottom: 10px;
        }
        .account-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
        }
        .account-name {
            font-family: monospace;
            font-size: 1.1em;
            color: #7fdbff;
        }
        .account-name.system { color: #f39c12; }
        .account-name.sandbox { color: #4ecdc4; }
        .account-meta { color: #888; font-size: 0.9em; margin-left: 10px; }
        .account-rules {
            font-family: monospace;
            font-size: 0.85em;
            color: #888;
            margin-top: 8px;
            padding-left: 10px;
        }
        .account-rule { padding: 2px 0; }
        .account-expired { color: #c0392b; }
        .sandbox-display { color: #aaa; font-size: 0.9em; }
        .btn-group { display: flex; gap: 5px; }
        .btn-small { padding: 6px 12px; font-size: 0.85em; }
        .request-item { background: #1a2744; border-left: 3px solid #f39c12; }
        .inline-editor {
            margin-top: 15px;
            padding-top: 15px;
            border-top: 1px solid #333;
        }
        .acl-editor {
            background: #0f1729;
            border-radius: 6px;
            overflow: hidden;
        }
        .acl-header {
            display: grid;
            grid-template-columns: 1fr 40px 40px 40px 50px;
            gap: 5px;
            padding: 10px 15px;
            background: #16213e;
            font-weight: bold;
            font-size: 0.9em;
            color: #888;
        }
        .acl-row {
            display: grid;
            grid-template-columns: 1fr 40px 40px 40px 50px;
            gap: 5px;
            padding: 10px 15px;
            border-top: 1px solid #1a1a2e;
            align-items: center;
        }
        .acl-row:hover { background: #16213e; }
        .acl-coord {
            font-family: monospace;
            font-size: 0.95em;
            color: #7fdbff;
            word-break: break-all;
        }
        .perm-btn {
            width: 32px;
            height: 32px;
            border: none;
            border-radius: 4px;
            cursor: pointer;
            font-family: monospace;
            font-weight: bold;
            font-size: 1em;
        }
        .perm-grant { background: #27ae60; color: white; }
        .perm-deny { background: #c0392b; color: white; }
        .perm-inherit { background: #555; color: #888; }
        .perm-btn:hover { opacity: 0.8; }
        .remove-btn {
            width: 32px;
            height: 32px;
            border: none;
            border-radius: 4px;
            cursor: pointer;
            background: transparent;
            color: #666;
            font-size: 1.2em;
        }
        .remove-btn:hover { background: #c0392b; color: white; }
        .add-row {
            display: grid;
            grid-template-columns: 1fr 40px 40px 40px 50px;
            gap: 5px;
            padding: 15px;
            background: #16213e;
            align-items: center;
        }
        .add-row input { width: 100%; padding: 8px; font-family: monospace; }
        .add-btn { width: 32px; height: 32px; padding: 0; font-size: 1.2em; }
        .editor-buttons { margin-top: 15px; display: flex; gap: 10px; }
    "#;

    let body = format!(r#"
    <div id="message"></div>

    <div class="card">
        <h2>Account Management</h2>
        <p style="color: #888; font-size: 0.9em; margin-top: 0;">
            Manage ring1 accounts and their ACL rules. System accounts cannot be deleted.
        </p>

        <div class="section-title">System Accounts</div>
        <div id="systemAccounts">Loading...</div>

        <div class="section-title">Site Sandboxes (HAVI-managed)</div>
        <div id="sandboxAccounts"><p class="empty">None</p></div>

        <div class="section-title">Custom Accounts</div>
        <div id="customAccounts"><p class="empty">None</p></div>

        <div class="section-title">Pending Requests</div>
        <div id="pendingRequests"><p class="empty">None</p></div>

        <div style="margin-top: 20px;">
            <button onclick="loadAll()" class="secondary">Refresh</button>
        </div>
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
    render_admin_page("Group Membership", "Groups", "", r#"
    <div class="card">
        <h2>Group Management</h2>
        <p class="empty">Group membership management coming soon.</p>
        <p style="color: #888; font-size: 0.9em;">
            Group setup: <code style="color: #7fdbff;">//<em>group</em>/admin/setup/|</code><br>
            Membership: <code style="color: #7fdbff;">//<em>group</em>/admin/members/|/seal/&lt;key&gt;</code>
        </p>
    </div>"#)
}

/// Render the Anyone account ACL editor page.
fn render_anyone_page() -> String {
    let anyone_js = include_str!("../js/havi-anyone.js");
    let extra_css = r#"
        .acl-editor { background: #0f1729; border-radius: 6px; overflow: hidden; }
        .acl-header {
            display: grid;
            grid-template-columns: 1fr 40px 40px 40px 50px;
            gap: 5px;
            padding: 10px 15px;
            background: #16213e;
            font-weight: bold;
            font-size: 0.9em;
            color: #888;
        }
        .acl-row {
            display: grid;
            grid-template-columns: 1fr 40px 40px 40px 50px;
            gap: 5px;
            padding: 10px 15px;
            border-top: 1px solid #1a1a2e;
            align-items: center;
        }
        .acl-row:hover { background: #16213e; }
        .acl-coord { font-family: monospace; font-size: 0.95em; color: #7fdbff; word-break: break-all; }
        .perm-btn {
            width: 32px; height: 32px; border: none; border-radius: 4px;
            cursor: pointer; font-family: monospace; font-weight: bold; font-size: 1em;
        }
        .perm-grant { background: #27ae60; color: white; }
        .perm-deny { background: #c0392b; color: white; }
        .perm-inherit { background: #555; color: #888; }
        .perm-btn:hover { opacity: 0.8; }
        .remove-btn {
            width: 32px; height: 32px; border: none; border-radius: 4px;
            cursor: pointer; background: transparent; color: #666; font-size: 1.2em;
        }
        .remove-btn:hover { background: #c0392b; color: white; }
        .add-row {
            display: grid;
            grid-template-columns: 1fr 40px 40px 40px 50px;
            gap: 5px; padding: 15px; background: #16213e; align-items: center;
        }
        .add-row input { width: 100%; padding: 8px; font-family: monospace; }
        .add-btn { width: 32px; height: 32px; padding: 0; font-size: 1.2em; }
        .save-section { margin-top: 20px; display: flex; gap: 10px; align-items: center; }
        .dirty-indicator { color: #f39c12; font-size: 0.9em; }
        .acl-tree details { margin-left: 0; }
        .acl-tree details details { margin-left: 20px; }
        .acl-tree summary { cursor: pointer; list-style: none; padding: 0; }
        .acl-tree summary::-webkit-details-marker { display: none; }
        .acl-tree summary::before {
            content: '\25B6  ';
            display: inline-block; width: 15px; font-size: 0.7em;
            transition: transform 0.2s; color: #666;
        }
        details[open] > summary::before { transform: rotate(90deg); }
        .acl-tree .leaf-rule { margin-left: 15px; }
        .acl-tree .acl-row { display: inline-grid; width: calc(100% - 15px); }
    "#;

    let body = format!(r#"
    <div id="message"></div>

    <div class="card">
        <h2>Access Rules</h2>
        <p style="color: #888; font-size: 0.9em; margin-top: 0;">
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
                ⚠ Unsaved changes
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
        .request-card {
            background: #0f1729;
            border-radius: 6px;
            padding: 15px;
            margin-bottom: 10px;
            border-left: 3px solid #f39c12;
        }
        .request-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 10px;
        }
        .request-ring1 { font-family: monospace; font-size: 1.1em; color: #7fdbff; }
        .request-cmd {
            font-family: monospace; font-size: 0.9em; color: #f39c12;
            background: #1a2744; padding: 2px 8px; border-radius: 4px;
        }
        .request-detail {
            font-family: monospace; font-size: 0.85em; color: #888;
            margin: 5px 0; word-break: break-all;
        }
        .btn-group { display: flex; gap: 5px; }
        .btn-small { padding: 6px 12px; font-size: 0.85em; }
        .status-indicator {
            display: inline-block; width: 8px; height: 8px;
            border-radius: 50%; margin-right: 8px;
        }
        .status-watching { background: #27ae60; animation: pulse 2s infinite; }
        .status-error { background: #c0392b; }
        @keyframes pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.4; }
        }
    "#;

    let body = format!(r#"
    <div id="message"></div>

    <div class="card">
        <h2><span id="watchIndicator" class="status-indicator status-watching"></span>Pending Proxy Requests</h2>
        <p style="color: #888; font-size: 0.9em; margin-top: 0;">
            Ring1 accounts submit proxy requests here. Approve to execute the command via ring0 and return the result.
        </p>
        <div id="requestsList"><p class="empty">Scanning...</p></div>
        <div style="margin-top: 15px;">
            <button onclick="scanRequests()" class="secondary">Refresh</button>
        </div>
    </div>

    <script>
{ring0_js}
    </script>"#,
        ring0_js = ring0_js
    );

    render_admin_page("Ring0 Proxy", "Ring0", extra_css, &body)
}

/// Render the services page.
fn render_services_page() -> String {
    let services_js = include_str!("../js/havi-services.js");
    let extra_css = r#"
        .btn-small { padding: 6px 12px; font-size: 0.85em; }
    "#;

    let body = format!(r#"
    <div id="message"></div>

    <div class="card">
        <h2>Pylon Status</h2>
        <div class="status">
            <div class="status-item">
                <div class="status-value" id="pylonStatus">checking...</div>
                <div class="status-label">Connection</div>
            </div>
        </div>
    </div>

    <div class="card">
        <h2>Services</h2>
        <p style="color: #888; font-size: 0.9em; margin-top: 0;">
            Managed services: hpprd, lokid, unlokid, hppr-fs.
        </p>
        <div id="servicesList"><p class="empty">Loading...</p></div>
        <div style="margin-top: 15px;">
            <button onclick="location.reload()" class="secondary">Refresh</button>
        </div>
    </div>

    <script>
{services_js}
    </script>"#,
        services_js = services_js
    );

    render_admin_page("Services", "Services", extra_css, &body)
}

/// Handle services API requests (proxied to pylon).
fn handle_services_api(path: &str) -> String {
    // Parse query string from path
    let query = path.split('?').nth(1).unwrap_or("");
    let params: Vec<(&str, &str)> = query.split('&')
        .filter_map(|p| p.split_once('='))
        .collect();

    let cmd = params.iter().find(|(k, _)| *k == "cmd").map(|(_, v)| *v).unwrap_or("status");
    let service = params.iter().find(|(k, _)| *k == "service").map(|(_, v)| *v);

    let mut client = match crate::pylon::PylonClient::try_connect(&crate::config::repo_dir()) {
        Some(c) => c,
        None => {
            return serde_json::json!({"error": "Pylon is not running. Start pylon first."}).to_string();
        }
    };

    match cmd {
        "status" => {
            match client.status() {
                Ok(services) => {
                    let list: Vec<serde_json::Value> = services.iter().map(|s| {
                        serde_json::json!({
                            "name": s.name,
                            "state": s.state,
                            "pid": s.pid,
                            "port": s.port,
                        })
                    }).collect();
                    serde_json::json!({"services": list}).to_string()
                }
                Err(e) => serde_json::json!({"error": e}).to_string(),
            }
        }
        "start" => {
            let Some(name) = service else {
                return serde_json::json!({"error": "missing service parameter"}).to_string();
            };
            match client.start_service(name) {
                Ok(()) => serde_json::json!({"ok": true}).to_string(),
                Err(e) => serde_json::json!({"error": e}).to_string(),
            }
        }
        "stop" => {
            let Some(name) = service else {
                return serde_json::json!({"error": "missing service parameter"}).to_string();
            };
            match client.stop_service(name) {
                Ok(()) => serde_json::json!({"ok": true}).to_string(),
                Err(e) => serde_json::json!({"error": e}).to_string(),
            }
        }
        _ => serde_json::json!({"error": format!("unknown command: {}", cmd)}).to_string(),
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
