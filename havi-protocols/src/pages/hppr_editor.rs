/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR Editor page handler.
//!
//! Handles hppr-editor:// URLs for local content editing.
//! URL format: hppr-editor://group/app/path

use std::sync::Arc;

use crate::PageResponse;
use crate::client::{get_admin_credentials, HpprdClientAsync};
use crate::credentials::CredentialStoreHandle;
use crate::url::HAVIAddress;
use crate::util::html_escape;

/// Handle an hppr-editor:// URL request.
pub async fn handle_request(
    url: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    let address = match HAVIAddress::parse(url) {
        Ok(u) => u,
        Err(e) => {
            return PageResponse::html(render_editor_error(&format!("Invalid coordinate: {}", e)));
        }
    };

    let parts = address.parts();
    let urc = address.urc_string();

    if parts.group.is_empty() || parts.app.is_empty() {
        return PageResponse::html(render_editor_error("Group and app must not be empty"));
    }

    // Fetch current content via ring0
    let current_content = match credential_store.get_admin() {
        Some(cred) => {
            let content_result = client
                .get_authenticated(&urc, &cred.ring1_name, &cred.token().to_string())
                .await;
            match content_result {
                Ok((_, body)) => match String::from_utf8(body) {
                    Ok(s) => s,
                    Err(_) => {
                        return PageResponse::html(render_editor_error(
                            "Content is not valid UTF-8 and cannot be edited as text",
                        ));
                    },
                },
                Err(_) => String::new(),
            }
        }
        None => String::new(),
    };

    let html = render_editor_html(&parts.group, &parts.app, &parts.location, &current_content, &urc);
    let (ring1_name, token) = get_admin_credentials();
    PageResponse::html(html).with_admin_credentials(ring1_name, token)
}

/// Render editor HTML template with single Save button.
fn render_editor_html(group: &str, app: &str, location: &str, content: &str, urc: &str) -> String {
    let default_headers = format!("Group: {}\nApp: {}\nLocation: {}", group, app, location);

    let extra_css = r#"
        .toolbar {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 15px;
            flex-wrap: wrap;
            gap: 10px;
        }
        h1 { font-size: 1.2em; margin: 0; }
        .urc { color: #7fdbff; font-family: monospace; font-size: 0.9em; }
        .buttons { display: flex; gap: 8px; flex-wrap: wrap; }
        button {
            padding: 8px 16px;
            font-size: 0.95em;
            border: none;
            border-radius: 6px;
            cursor: pointer;
        }
        .save { background: #27ae60; color: white; }
        .save:hover { background: #2ecc71; }
        button:disabled { background: #555; cursor: not-allowed; }
        .cancel { background: #555; color: white; }
        .cancel:hover { background: #666; }
        .editor-container {
            flex: 1;
            display: flex;
            flex-direction: column;
            min-height: 400px;
            gap: 10px;
        }
        .section-label { color: #888; font-size: 0.85em; margin-bottom: 4px; }
        .headers-section { flex-shrink: 0; }
        .data-section { flex: 1; display: flex; flex-direction: column; }
        textarea {
            width: 100%;
            padding: 12px;
            font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
            font-size: 14px;
            line-height: 1.5;
            background: #16213e;
            color: #eee;
            border: 2px solid #333;
            border-radius: 8px;
            resize: vertical;
            tab-size: 4;
        }
        textarea:focus { outline: none; border-color: #4ecdc4; }
        #headers { min-height: 80px; }
        #data { flex: 1; min-height: 200px; }
        .status {
            margin-top: 10px;
            padding: 10px;
            border-radius: 6px;
            display: none;
        }
        .status.error {
            display: block;
            background: #2d1f1f;
            border-left: 4px solid #ff6b6b;
            color: #ff6b6b;
        }
        .status.saving {
            display: block;
            background: #1f2d1f;
            border-left: 4px solid #27ae60;
            color: #27ae60;
        }
    "#;

    let editor_js = include_str!("../js/hppr-editor.js");
    let script = format!("\n    <script>{}</script>", editor_js);

    let body = format!(
        r#"    <div class="toolbar">
        <div>
            <h1>Edit Content</h1>
            <span class="urc">{urc}</span>
        </div>
        <div class="buttons">
            <button class="cancel" onclick="history.back()">Cancel</button>
            <button class="save" id="saveBtn" onclick="saveContent()">Save</button>
        </div>
    </div>
    <div class="editor-container">
        <div class="headers-section">
            <div class="section-label">Headers</div>
            <textarea id="headers" spellcheck="false">{default_headers}</textarea>
        </div>
        <div class="data-section">
            <div class="section-label">Data</div>
            <textarea id="data" spellcheck="false">{content}</textarea>
        </div>
    </div>
    <div class="status" id="status"></div>
{script}"#,
        urc = html_escape(urc),
        default_headers = html_escape(&default_headers),
        content = html_escape(content),
        script = script,
    );

    crate::page_shell::render_page(&format!("Edit {} - HAVI", html_escape(urc)), extra_css, &body)
}

/// Render editor error page.
fn render_editor_error(error: &str) -> String {
    let css = r#"
        .error-container { max-width: 600px; margin: 80px auto; }
        h1 { color: #ff6b6b; }
        .error {
            background: #2d1f1f;
            padding: 20px;
            border-radius: 8px;
            border-left: 4px solid #ff6b6b;
            font-family: monospace;
            color: #ff6b6b;
        }
    "#;

    let body = format!(
        r#"    <div class="error-container">
        <h1>Editor Error</h1>
        <div class="error">{error}</div>
        <p><button onclick="history.back()">Go Back</button></p>
    </div>"#,
        error = html_escape(error)
    );

    crate::page_shell::render_page("Editor Error - HAVI", css, &body)
}
