/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared utilities for HPPR protocol handlers.

use std::sync::Arc;

use hppr_client::env_target::ViaSpec;

use crate::client::HpprdClientAsync;

// Re-export credential types from credentials module
pub use crate::credentials::{CredentialStoreHandle, global_credential_store};

/// Infer MIME type from file extension.
pub fn mime_from_path(path: &str) -> &'static str {
    if path.ends_with(".html") || path.ends_with(".htm") {
        "text/html"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".js") || path.ends_with(".mjs") {
        "application/javascript"
    } else if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".gif") {
        "image/gif"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".wasm") {
        "application/wasm"
    } else if path.ends_with(".txt") {
        "text/plain"
    } else if path.ends_with(".xml") {
        "application/xml"
    } else if path.ends_with(".md") {
        "text/markdown"
    } else {
        "application/octet-stream"
    }
}

/// Escape HTML special characters for safe rendering.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Render a styled error page with consistent dark theme.
///
/// # Arguments
/// - `title`: The page title and h1 heading (e.g., "HPPR Error", "Preview Error")
/// - `message`: The error message to display
/// - `hint`: Optional hint text (e.g., "Expected URL format: ...")
///
/// # Returns
/// HTML string for the error page
pub fn render_error_page(title: &str, message: &str, hint: Option<&str>) -> String {
    use crate::page_shell::render_page;

    let hint_html = hint
        .map(|h| format!(r#"<p class="hint">{}</p>"#, html_escape(h)))
        .unwrap_or_default();

    let css = r#"
        body { max-width: 600px; margin: 100px auto; }
        h1 { color: #ff6b6b; }
        .error {
            background: #2d1f1f;
            padding: 20px;
            border-radius: 8px;
            border-left: 4px solid #ff6b6b;
            font-family: monospace;
        }
        .hint { color: #888; margin-top: 16px; }
        .hint code {
            color: #7fdbff;
            background: #2d2d4e;
            padding: 2px 6px;
            border-radius: 3px;
        }
    "#;

    let body = format!(
        "    <h1>{title}</h1>\n    <div class=\"error\">{message}</div>\n    {hint_html}",
        title = html_escape(title),
        message = html_escape(message),
        hint_html = hint_html,
    );

    render_page(&html_escape(title), css, &body)
}

/// Resolve route endpoint for a group/app via admin route lookup.
///
/// Looks up the route packet in the home repo using admin credentials.
/// Falls back to the default repo endpoint if:
/// - Group or app is empty
/// - No admin credential available
/// - Admin identity lookup fails
/// - Route lookup fails or has no upstream address
///
/// Returns `(endpoint, upstream_verification_key)`.
pub async fn resolve_route_endpoint(
    group: &str,
    app: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> (ViaSpec, Option<String>) {
    let repo_target = hppr_client::repo_target().clone();

    if group.is_empty() || app.is_empty() {
        return (repo_target, None);
    }

    let cred = match credential_store.get_admin() {
        Some(c) => c,
        None => {
            log::debug!("No admin credential for route lookup, falling back to repo");
            return (repo_target, None);
        }
    };

    let account = cred.ring1_name.clone();
    let token = cred.token().to_string();

    let repo_vkey = match repo_client.get_admin_identity(&account, &token).await {
        Ok(key) => key,
        Err(e) => {
            log::debug!("Failed to get admin identity for route lookup: {}, falling back to repo", e);
            return (repo_target, None);
        }
    };

    match repo_client.get_route(group, app, &repo_vkey, &account, &token).await {
        Ok(route_info) => {
            let endpoint = route_info.upstream.unwrap_or_else(|| {
                log::debug!("Route for {}/{} has no upstream, falling back to repo", group, app);
                repo_target.clone()
            });
            (endpoint, route_info.upstream_verification_key)
        },
        Err(e) => {
            log::debug!("No route for {}/{}: {}, falling back to repo", group, app, e);
            (repo_target, None)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("hello"), "hello");
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
        assert_eq!(
            html_escape("<a href=\"test\">link & text</a>"),
            "&lt;a href=&quot;test&quot;&gt;link &amp; text&lt;/a&gt;"
        );
    }

    #[test]
    fn test_mime_from_path() {
        // HTML
        assert_eq!(mime_from_path("index.html"), "text/html");
        assert_eq!(mime_from_path("page.htm"), "text/html");
        // CSS
        assert_eq!(mime_from_path("style.css"), "text/css");
        // JavaScript
        assert_eq!(mime_from_path("app.js"), "application/javascript");
        assert_eq!(mime_from_path("module.mjs"), "application/javascript");
        // JSON
        assert_eq!(mime_from_path("data.json"), "application/json");
        // Images
        assert_eq!(mime_from_path("logo.png"), "image/png");
        assert_eq!(mime_from_path("photo.jpg"), "image/jpeg");
        assert_eq!(mime_from_path("photo.jpeg"), "image/jpeg");
        assert_eq!(mime_from_path("anim.gif"), "image/gif");
        assert_eq!(mime_from_path("icon.svg"), "image/svg+xml");
        // Other
        assert_eq!(mime_from_path("module.wasm"), "application/wasm");
        assert_eq!(mime_from_path("readme.txt"), "text/plain");
        assert_eq!(mime_from_path("config.xml"), "application/xml");
        assert_eq!(mime_from_path("docs.md"), "text/markdown");
        // Fallback
        assert_eq!(mime_from_path("file.unknown"), "application/octet-stream");
        assert_eq!(mime_from_path("noext"), "application/octet-stream");
    }
}
