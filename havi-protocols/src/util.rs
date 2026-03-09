/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared utilities for HPPR protocol handlers.

use std::sync::Arc;

use hppr_client::ViaSpec;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use pulldown_cmark::{Options, Parser, html};

use crate::client::HpprdClientAsync;

// Re-export credential types from credentials module
pub use crate::credentials::{CredentialStoreHandle, global_credential_store};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteEndpointSource {
    HomeFallback,
    Routed,
}

/// Characters that need encoding in URL path segments.
pub const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'?')
    .add(b'{')
    .add(b'}');

/// Convert markdown content to HTML with styling.
pub fn markdown_to_html(markdown: &[u8], title: &str) -> Result<Vec<u8>, String> {
    let markdown_str = std::str::from_utf8(markdown)
        .map_err(|e| format!("markdown body is not valid UTF-8: {}", e))?;

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(markdown_str, options);
    let mut html_body = String::new();
    html::push_html(&mut html_body, parser);

    let css = r#"
        body { max-width: 800px; margin: 40px auto; line-height: 1.6; }
        code { background: #2d2d4e; padding: 2px 6px; border-radius: 3px; }
        pre { background: #2d2d4e; padding: 16px; border-radius: 8px; overflow-x: auto; }
        table { border-collapse: collapse; width: 100%; margin: 20px 0; }
        th, td { border: 1px solid #444; padding: 8px 12px; text-align: left; }
        th { background: #2d2d4e; }
    "#;

    let page = crate::page_shell::render_page(title, css, &html_body);
    Ok(page.into_bytes())
}

/// Append a requested location to a root path, handling slashes.
pub fn append_location(root: &str, requested_location: &str) -> String {
    let root_base = root.trim_end_matches('/');
    let requested = requested_location.trim_matches('/');
    if requested.is_empty() {
        root_base.to_string()
    } else {
        format!("{}/{}", root_base, requested)
    }
}

/// Derive a verifying key from a signing key.
pub fn signing_to_verifying_key(signing_key: &str) -> Result<String, String> {
    let (tc, sk_bytes) = hppr_packet::crypto::t_b64a_h3_decode(signing_key)
        .map_err(|e| format!("Invalid route signing key: {}", e))?;
    if tc != '&' {
        return Err("Invalid route signing key: expected '&' prefix".to_string());
    }
    hppr_packet::crypto::get_verification_key(&sk_bytes)
        .map_err(|e| format!("Failed to derive route verification key: {}", e))
}

/// Render a directory listing as HTML.
///
/// Each entry is a link. `path_display` is shown in the heading.
/// If `show_dir_suffix` is true, directory entries (ending in `/`) get their
/// trailing slash displayed.
pub fn render_directory_listing(
    path_display: &str,
    entries: &[(String, bool)],
) -> String {
    let links: String = entries
        .iter()
        .map(|(name, is_dir)| {
            let display = if *is_dir {
                format!("{}/", name)
            } else {
                name.clone()
            };
            let encoded = utf8_percent_encode(&display, PATH_SEGMENT_ENCODE_SET).to_string();
            format!(
                r#"<li><a href="./{}">{}</a></li>"#,
                encoded,
                html_escape(&display),
            )
        })
        .collect::<Vec<_>>()
        .join("\n            ");

    let css = r#"
        body { max-width: 800px; margin: 40px auto; }
        h1 { border-bottom: 2px solid #4ecdc4; padding-bottom: 10px; }
        ul { list-style: none; padding: 0; }
        li { padding: 8px 0; border-bottom: 1px solid #333; }
        a { font-family: monospace; font-size: 1.1em; }
        a:hover { color: #fff; }
        .empty { color: #888; font-style: italic; }
    "#;

    let escaped_path = html_escape(path_display);
    let content = if entries.is_empty() {
        r#"<p class="empty">(empty)</p>"#.to_string()
    } else {
        format!("<ul>\n            {}\n        </ul>", links)
    };

    let body = format!("    <h1>Index of {}</h1>\n    {}", escaped_path, content);
    crate::page_shell::render_page(&format!("Index of {}", escaped_path), css, &body)
}

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
/// Returns `(endpoint, upstream_verification_key, source)`.
pub async fn resolve_route_endpoint(
    group: &str,
    app: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> (ViaSpec, Option<String>, RouteEndpointSource) {
    let repo_target = repo_client.target();

    if group.is_empty() || app.is_empty() {
        return (repo_target, None, RouteEndpointSource::HomeFallback);
    }

    if credential_store.get_admin().is_none() {
        log::debug!("No admin credential for route lookup, falling back to repo");
        return (repo_target, None, RouteEndpointSource::HomeFallback);
    }

    let repo_vkey = match repo_client.get_admin_identity().await {
        Ok(key) => key,
        Err(e) => {
            log::debug!(
                "Failed to get admin identity for route lookup: {}, falling back to repo",
                e
            );
            return (repo_target, None, RouteEndpointSource::HomeFallback);
        },
    };

    match repo_client
        .get_route(group, app, &repo_vkey)
        .await
    {
        Ok(route_info) => {
            let endpoint = route_info.upstream.unwrap_or_else(|| {
                log::debug!(
                    "Route for {}/{} has no upstream, falling back to repo",
                    group,
                    app
                );
                repo_target.clone()
            });
            (
                endpoint,
                route_info.upstream_verification_key,
                RouteEndpointSource::Routed,
            )
        },
        Err(e) => {
            log::debug!("No route for {}/{}: {}", group, app, e);

            match hppr_client::tokio::index::lookup_bootstrap_index_if_indexed(group, app).await {
                Ok(Some(index)) => {
                    let mut headers = format!(
                        "Group: repo\nApp: admin\nLocation: route/{group}/{app}\nSeal-By: oldest\nUpstream: {}\n",
                        index.upstream
                    );
                    if let Some(vkey) = &index.upstream_verification_key {
                        headers.push_str(&format!("Upstream-Verification-Key: {}\n", vkey));
                    }

                    let add_args = hppr_client::build_add_args(headers.as_bytes(), Some(&[]));
                    match repo_client.add(&add_args).await {
                        Ok(_) => log::info!(
                            "Installed bootstrap route for //{}/{} -> {}",
                            group,
                            app,
                            index.upstream
                        ),
                        Err(err) => log::info!(
                            "Bootstrap route resolved for //{}/{}, install failed (continuing): {}",
                            group,
                            app,
                            err
                        ),
                    }

                    (
                        index.upstream,
                        index.upstream_verification_key,
                        RouteEndpointSource::Routed,
                    )
                }
                Ok(None) => (repo_target, None, RouteEndpointSource::HomeFallback),
                Err(err) => {
                    log::info!(
                        "Bootstrap lookup failed for //{}/{}: {}, falling back to repo",
                        group,
                        app,
                        err
                    );
                    (repo_target, None, RouteEndpointSource::HomeFallback)
                }
            }
        }
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
