/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared utilities for HPPR protocol handlers.

use std::sync::Arc;

use hppr_client::ViaSpec;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use pulldown_cmark::{Options, Parser, html};

use super::client::HpprdClientAsync;

// Re-export credential types from credentials module
pub use super::credentials::{CredentialStoreHandle, global_credential_store};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteEndpointSource {
    HomeFallback,
    LocalRoute,
    PublicNetwork,
    DirectVia,
}

#[derive(Clone, Debug)]
struct ExactGroupRoute {
    endpoint: ViaSpec,
    route_authority_key: Option<String>,
    upstream_verification_key: Option<String>,
    home_app: Option<String>,
    source: RouteEndpointSource,
}

impl RouteEndpointSource {
    pub fn as_str(self) -> &'static str {
        match self {
            RouteEndpointSource::HomeFallback => "home-fallback",
            RouteEndpointSource::LocalRoute => "local-route",
            RouteEndpointSource::PublicNetwork => "public-network",
            RouteEndpointSource::DirectVia => "direct-via",
        }
    }
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

    let page = crate::pages::page_shell::render_page(title, css, &html_body);
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

pub fn shadow_group(group: &str) -> String {
    format!("~{}", group)
}

pub fn shadow_root(group: &str, app: &str) -> String {
    format!("//{}/{}", shadow_group(group), app)
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
    crate::pages::page_shell::render_page(&format!("Index of {}", escaped_path), css, &body)
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
    } else if path.ends_with(".mp4") || path.ends_with(".m4v") {
        "video/mp4"
    } else if path.ends_with(".m4a") {
        "audio/mp4"
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
    use crate::pages::page_shell::render_page;

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

fn report_route_resolution(
    group: &str,
    app: &str,
    source: RouteEndpointSource,
    endpoint: &ViaSpec,
    upstream_verification_key: Option<&str>,
    note: Option<&str>,
) {
    if group.is_empty() || app.is_empty() {
        return;
    }

    let mut line = format!(
        "[havi] route resolve: //{}/{} source={} endpoint={}",
        group,
        app,
        source.as_str(),
        endpoint
    );
    if let Some(vkey) = upstream_verification_key {
        line.push_str(" upstream_vkey=");
        line.push_str(vkey);
    }
    if let Some(note) = note {
        line.push_str(" note=");
        line.push_str(note);
    }
    eprintln!("{}", line);
}

fn exact_groups(group: &str) -> Result<Vec<String>, String> {
    let labels = hppr_client::network::split_group_labels(group).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(labels.len());
    let mut current = String::new();
    for label in labels {
        if current.is_empty() {
            current = label.to_string();
        } else {
            current = format!("{}.{}", label, current);
        }
        out.push(current.clone());
    }
    Ok(out)
}

async fn fetch_public_packet_via(via: &ViaSpec, urc: &str) -> Result<hppr_client::Packet, String> {
    let client = Arc::new(HpprdClientAsync::new_with_signer(via.clone(), hppr_client::Signer::anyone()));
    client.get_packet_authenticated(urc).await
}

async fn resolve_exact_group_route(
    group: &str,
    app: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> Result<ExactGroupRoute, String> {
    let repo_target = repo_client.target();
    if group.is_empty() || app.is_empty() {
        return Ok(ExactGroupRoute {
            endpoint: repo_target,
            route_authority_key: None,
            upstream_verification_key: None,
            home_app: None,
            source: RouteEndpointSource::HomeFallback,
        });
    }

    let public_name = hppr_client::is_public_name(group, app);
    let mut root_override = None;
    let mut repo_vkey = None;

    if credential_store.get_admin().is_some() {
        match repo_client.get_admin_identity().await {
            Ok(vkey) => {
                repo_vkey = Some(vkey.clone());
                root_override = repo_client.get_local_route_group("u", &vkey).await.ok();
            }
            Err(e) => {
                log::debug!("Failed to get admin identity for local route lookup: {}", e);
            }
        }
    }

    let root_config = hppr_client::RouteRootConfig::load().map_err(|e| e.to_string())?;
    let mut current_target = if let Some(root) = &root_override {
        Some(root.upstream.clone())
    } else if public_name {
        Some(root_config.server.clone())
    } else {
        None
    };
    let mut route_authority_key = if let Some(root) = &root_override {
        Some(root.route_authority_key.clone())
    } else if public_name {
        Some(root_config.pubkey.clone())
    } else {
        None
    };
    let mut upstream_verification_key = None;
    let mut home_app = root_override.as_ref().and_then(|root| root.home_app.clone());
    let mut parent_group = "u".to_string();
    let mut used_local = root_override.is_some();

    let exact_groups = exact_groups(group)?;
    for exact_group in exact_groups.iter().cloned() {
        if let Some(repo_vkey) = repo_vkey.as_deref()
            && let Ok(local_group) = repo_client.get_local_route_group(&exact_group, repo_vkey).await
        {
            used_local = true;
            current_target = Some(local_group.upstream.clone());
            route_authority_key = Some(local_group.route_authority_key.clone());
            upstream_verification_key = local_group.upstream_verification_key.clone();
            home_app = local_group.home_app.clone();
            parent_group = exact_group;
            continue;
        }

        if !public_name {
            break;
        }
        let target = current_target
            .as_ref()
            .ok_or_else(|| format!("public route lookup failed for //{}/{}", group, app))?;
        let expected = route_authority_key
            .as_ref()
            .ok_or_else(|| format!("public route lookup failed for //{}/{}", group, app))?;
        let child_label = if parent_group == "u" && !exact_group.contains('.') {
            exact_group.clone()
        } else {
            exact_group
                .strip_suffix(&format!(".{}", parent_group))
                .unwrap_or(&exact_group)
                .trim_end_matches('.')
                .to_string()
        };
        let urc = format!("//{}/route/group/{}", parent_group, child_label);
        let packet = fetch_public_packet_via(target, &urc)
            .await
            .map_err(|_| format!("public route lookup failed for //{}/{}", group, app))?;
        let record = hppr_client::network::parse_group_record(&packet, expected, &parent_group, &child_label)
            .map_err(|e| e.to_string())?;
        current_target = Some(record.upstream.clone());
        route_authority_key = Some(record.route_authority_key.clone());
        upstream_verification_key = record.upstream_verification_key.clone();
        home_app = record.home_app.clone();
        parent_group = record.resolved_group;
    }

    if public_name && !exact_groups.is_empty() && parent_group != group {
        return Err(format!("public route lookup failed for //{}/{}", group, app));
    }

    if let (Some(endpoint), Some(route_authority_key)) = (current_target, route_authority_key) {
        return Ok(ExactGroupRoute {
            endpoint,
            route_authority_key: Some(route_authority_key),
            upstream_verification_key,
            home_app,
            source: if used_local {
                RouteEndpointSource::LocalRoute
            } else {
                RouteEndpointSource::PublicNetwork
            },
        });
    }

    Ok(ExactGroupRoute {
        endpoint: repo_target,
        route_authority_key: None,
        upstream_verification_key: None,
        home_app: None,
        source: RouteEndpointSource::HomeFallback,
    })
}

pub async fn resolve_group_home_app(
    group: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> Result<Option<String>, String> {
    let exact = resolve_exact_group_route(group, "home", repo_client, credential_store).await?;
    Ok(exact.home_app)
}

/// Resolve route endpoint for a group/app via local and public route records.
///
/// Returns `(endpoint, upstream_verification_key, content_authority_pin, source)`.
pub async fn resolve_route_endpoint(
    group: &str,
    app: &str,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> Result<(ViaSpec, Option<String>, Option<String>, RouteEndpointSource), String> {
    let repo_target = repo_client.target();
    let exact = resolve_exact_group_route(group, app, repo_client, credential_store).await?;
    let public_name = hppr_client::is_public_name(group, app);
    let mut local_app = None;

    if credential_store.get_admin().is_some() {
        match repo_client.get_admin_identity().await {
            Ok(vkey) => {
                local_app = repo_client.get_local_route_app(group, app, &vkey).await.ok();
            }
            Err(e) => {
                log::debug!("Failed to get admin identity for local route lookup: {}", e);
            }
        }
    }

    let public_app = if public_name {
        if let Some(route_authority_key) = exact.route_authority_key.as_ref() {
            let urc = format!("//{}/route/app/{}", group, app);
            match fetch_public_packet_via(&exact.endpoint, &urc).await {
                Ok(packet) => Some(
                    hppr_client::network::parse_app_record(
                        &packet,
                        std::slice::from_ref(route_authority_key),
                        group,
                        app,
                    )
                    .map_err(|e| e.to_string())?,
                ),
                Err(_) => None,
            }
        } else {
            None
        }
    } else {
        None
    };

    if public_name && local_app.is_none() && public_app.is_none() {
        report_route_resolution(
            group,
            app,
            RouteEndpointSource::PublicNetwork,
            &repo_target,
            None,
            Some("public-app-missing"),
        );
        return Err(format!("public route lookup failed for //{}/{}", group, app));
    }

    let content_authority = if let Some(local_app) = &local_app {
        local_app.content_authority.clone()
    } else if let Some(public_app) = &public_app {
        if public_app.content_authority.is_some() {
            public_app.content_authority.clone()
        } else if public_name && group != "u" {
            hppr_client::lookup_route_if_public_async(group, app)
                .await
                .ok()
                .flatten()
                .and_then(|lookup| lookup.content_authority)
        } else {
            None
        }
    } else {
        None
    };

    let endpoint = local_app
        .as_ref()
        .and_then(|r| r.upstream.clone())
        .or_else(|| public_app.as_ref().and_then(|r| r.upstream.clone()))
        .or_else(|| {
            if group == "u" || matches!(exact.source, RouteEndpointSource::HomeFallback) {
                None
            } else if public_name && public_app.is_none() && local_app.is_some() {
                Some(exact.endpoint.clone())
            } else if public_name {
                None
            } else {
                Some(exact.endpoint.clone())
            }
        });
    let upstream_verification_key = local_app
        .as_ref()
        .and_then(|r| r.upstream_verification_key.clone())
        .or_else(|| public_app.as_ref().and_then(|r| r.upstream_verification_key.clone()))
        .or_else(|| exact.upstream_verification_key.clone());

    if let Some(endpoint) = endpoint {
        report_route_resolution(
            group,
            app,
            exact.source,
            &endpoint,
            upstream_verification_key.as_deref(),
            None,
        );
        return Ok((endpoint, upstream_verification_key, content_authority, exact.source));
    }

    if public_name {
        report_route_resolution(
            group,
            app,
            RouteEndpointSource::PublicNetwork,
            &repo_target,
            None,
            Some("public-route-failed"),
        );
        return Err(format!("public route lookup failed for //{}/{}", group, app));
    }

    report_route_resolution(
        group,
        app,
        RouteEndpointSource::HomeFallback,
        &repo_target,
        None,
        Some("not-public-name"),
    );
    Ok((repo_target, None, None, RouteEndpointSource::HomeFallback))
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
        // Media
        assert_eq!(mime_from_path("clip.mp4"), "video/mp4");
        assert_eq!(mime_from_path("clip.m4v"), "video/mp4");
        assert_eq!(mime_from_path("track.m4a"), "audio/mp4");
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
