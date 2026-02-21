/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR page handler.
//!
//! Handles hppr:// URLs by fetching content from an HPPR daemon.
//! URL format: hppr://group/app/location
//! LIST mode: hppr://group/app/location/ (trailing slash)

use std::sync::Arc;

use hppr_client::env_target::{ViaSpec, parse_via};
use hppr_packet::chunk::{is_chunk_manifest, parse_chunk_manifest, ChunkKind};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use pulldown_cmark::{Parser, Options, html};

use crate::PageResponse;
use crate::client::HpprdClientAsync;
use crate::credentials::CredentialStoreHandle;
use crate::url::{HAVIAddress, via_url};
use crate::util::{html_escape, mime_from_path, resolve_route_endpoint};

/// Characters that need encoding in URL path segments
const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'?')
    .add(b'{')
    .add(b'}');

/// Mode of HPPR request
enum HpprMode {
    Get,
    List,
}

/// Check if an endpoint string refers to the home repo.
fn is_repo_endpoint(endpoint: &ViaSpec, repo_target: &ViaSpec) -> bool {
    endpoint == repo_target
}

/// Resolve a HAVIAddress to an endpoint, URC string, and upstream verification key.
async fn resolve_target(
    url: &HAVIAddress,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
    page_endpoint: Option<&ViaSpec>,
) -> Result<(ViaSpec, String, Option<String>), String> {
    let repo_target = hppr_client::repo_target().clone();

    let parts = url.parts();
    let location = url.location_with_slash();
    let urc = HAVIAddress::build_urc_string(&parts.group, &parts.app, &location);

    let (endpoint, upstream_key) = if let Some(endpoint) = url.endpoint_string() {
        if endpoint == "repo" {
            (repo_target, None)
        } else {
            let via = parse_via(&endpoint).map_err(|e| e.to_string())?;
            (via, None)
        }
    } else if let Some(endpoint) = page_endpoint {
        (endpoint.clone(), None)
    } else {
        let (endpoint, upstream_key) = resolve_route_endpoint(
            &parts.group,
            &parts.app,
            repo_client,
            credential_store,
        )
        .await;

        (endpoint, upstream_key)
    };

    // Use site-trust keys for automatic seal resolution
    if let Some(cred) = credential_store.get_admin() {
        let account = cred.ring1_name.clone();
        let token = cred.token().to_string();
        let trusted_keys = repo_client
            .get_site_trust_keys(&parts.group, &parts.app, &account, &token)
            .await;
        if let Some(first_key) = trusted_keys.first() {
            let sealed_urc = format!("{}/|/seal/{}", urc, first_key);
            log::debug!("Site-trust key found, resolving to: {}", sealed_urc);
            return Ok((endpoint, sealed_urc, upstream_key));
        }
    }

    Ok((endpoint, urc, upstream_key))
}

/// Detect mode from URL path
fn detect_mode(path: &str) -> HpprMode {
    if path.ends_with('/') {
        HpprMode::List
    } else {
        HpprMode::Get
    }
}

/// Convert markdown content to HTML with styling.
fn markdown_to_html(markdown: &[u8], title: &str) -> Result<Vec<u8>, String> {
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

/// Handle an hppr:// URL request.
pub async fn handle_request(
    url: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    log::info!("hppr::handle_request url={}", url);

    let address = match HAVIAddress::parse(url) {
        Ok(u) => u,
        Err(e) => {
            return PageResponse::error("Invalid URL", &e.to_string(), None);
        },
    };

    let parts = address.parts();
    let location = address.location_with_slash();

    // Direct endpoint redirect: when navigating to hppr://...{via:...} for the first time,
    // redirect to hppr-setup://...{via:...} if no route exists.
    if let Some(host) = address.endpoint_string() {
        if !parts.group.is_empty() && !parts.app.is_empty() {
            let route_exists = match credential_store.get_admin() {
                Some(cred) => {
                    let account = cred.ring1_name.clone();
                    let token = cred.token().to_string();
                    match client.get_admin_identity(&account, &token).await {
                        Ok(key) => {
                            client
                                .get_route(&parts.group, &parts.app, &key, &account, &token)
                                .await
                                .is_ok()
                        },
                        Err(_) => false,
                    }
                },
                None => false,
            };

            if !route_exists {
                let setup_coord = if location.is_empty() || location == "/" {
                    format!("hppr-setup://{}/{}/", parts.group, parts.app)
                } else {
                    format!("hppr-setup://{}/{}/{}", parts.group, parts.app, location)
                };
                let setup_url = via_url(&setup_coord, &host);
                let redirect_html = render_setup_redirect(&setup_url, &host, &parts.group, &parts.app);
                return PageResponse::html(redirect_html);
            }
        }
    }

    let (endpoint, urc, _upstream_key) =
        match resolve_target(&address, client, credential_store, None).await {
        Ok(r) => r,
        Err(e) => {
            return PageResponse::error("HPPR Error", &e, Some(&format!("URL: {}", url)));
        },
    };

    log::info!("hppr::handle_request resolved endpoint={} urc={}", endpoint, urc);
    let repo_target = hppr_client::repo_target().clone();
    let is_repo = is_repo_endpoint(&endpoint, &repo_target);

    let mode = detect_mode(&urc);

    match mode {
        HpprMode::Get => {
            handle_get(
                &endpoint,
                url,
                &urc,
                &parts.group,
                &parts.app,
                is_repo,
                client,
                credential_store,
            ).await
        },
        HpprMode::List => {
            handle_list(client, &endpoint, url, &urc).await
        },
    }
}

/// Reassemble chunk data from a chunk manifest by fetching each chunk blob by hash.
async fn reassemble_chunks(
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
    manifest: &hppr_packet::chunk::ChunkManifest,
) -> Result<Vec<u8>, String> {
    let cred = credential_store
        .get_admin()
        .ok_or("No admin credential for chunk fetch")?;
    let account = cred.ring1_name.clone();
    let token = cred.token().to_string();

    let mut result = Vec::with_capacity(manifest.total_length as usize);

    for chunk in &manifest.chunks {
        let hash_urc = format!("////{}", chunk.hash);
        let blob_packet = client
            .get_packet_authenticated(&hash_urc, &account, &token)
            .await
            .map_err(|e| format!("failed to fetch chunk {}: {}", chunk.hash, e))?;

        let chunk_data = match chunk.kind {
            ChunkKind::Blob => blob_packet.data().to_vec(),
            ChunkKind::Manifest => {
                // Nested manifest: parse sub-manifest and recurse
                let sub_headers: Vec<(String, String)> = blob_packet
                    .headers()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let sub_manifest = parse_chunk_manifest(&sub_headers)
                    .map_err(|e| format!("invalid sub-manifest: {e}"))?;
                Box::pin(reassemble_chunks(client, credential_store, &sub_manifest)).await?
            },
        };

        let expected = (chunk.end - chunk.start) as usize;
        if chunk_data.len() != expected {
            return Err(format!(
                "chunk {} has {} bytes, expected {}",
                chunk.hash,
                chunk_data.len(),
                expected,
            ));
        }
        result.extend_from_slice(&chunk_data);
    }

    Ok(result)
}

/// Handle GET requests.
async fn handle_get(
    endpoint: &ViaSpec,
    url: &str,
    urc: &str,
    group: &str,
    app: &str,
    is_repo: bool,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    log::info!("handle_get: urc={} endpoint={} is_repo={}", urc, endpoint, is_repo);

    if group.is_empty() || app.is_empty() {
        return PageResponse::error("HPPR Error", "Group and app are required", Some(&format!("URL: {}", url)));
    }

    // Build the appropriate client for the target endpoint.
    // For routed (non-repo) content, connect to the remote with Ring2 signer.
    let route_client: Option<Arc<HpprdClientAsync>> = if !is_repo {
        match credential_store
            .get_or_create_route_credential_async(group, client)
            .await
        {
            Ok(route_cred) => {
                let signer = hppr_client::Signer::ring2(group, route_cred.signing_key());
                Some(Arc::new(HpprdClientAsync::new_with_signer(
                    endpoint.clone(),
                    signer,
                )))
            },
            Err(e) => {
                log::warn!("No route credential for {}: {}, falling back to repo", group, e);
                None
            },
        }
    } else {
        None
    };
    let fetch_client = route_client.as_ref().unwrap_or(client);

    // Fetch packet via GET
    let cred = credential_store.get_admin();
    let fetch_result = fetch_client
        .get_packet_authenticated(urc, "", "")
        .await;

    let packet = match fetch_result {
        Ok(p) => p,
        Err(e) => {
            if e.contains("NOT_FOUND") {
                return render_not_found_response(url);
            }
            return PageResponse::error("HPPR Error", &e, Some(&format!("URL: {}", url)));
        },
    };

    // Chunk manifest detection and reassembly
    let headers_vec: Vec<(String, String)> = packet
        .headers()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let (content_type, body) = if is_chunk_manifest(&headers_vec) {
        let manifest = match parse_chunk_manifest(&headers_vec) {
            Ok(m) => m,
            Err(e) => {
                return PageResponse::error(
                    "Chunk Error",
                    &format!("invalid chunk manifest: {e}"),
                    Some(&format!("URL: {}", url)),
                );
            },
        };

        let ct = manifest
            .content_type
            .clone()
            .unwrap_or_else(|| "text/html".to_string());

        match reassemble_chunks(client, credential_store, &manifest).await {
            Ok(data) => (ct, data),
            Err(e) => {
                return PageResponse::error(
                    "Chunk Error",
                    &format!("chunk reassembly failed: {e}"),
                    Some(&format!("URL: {}", url)),
                );
            },
        }
    } else {
        let ct = packet
            .header("Content-Type")
            .unwrap_or("text/html")
            .to_string();
        (ct, packet.data().to_vec())
    };

    // Trust check for remote sealed content
    if !is_repo {
        // Check trust via site-trust keys
        if let Some(c) = &cred {
            let account = c.ring1_name.clone();
            let token = c.token().to_string();
            let trusted_keys = client
                .get_site_trust_keys(group, app, &account, &token)
                .await;
            if trusted_keys.is_empty() {
                let setup_coord = format!("hppr-setup://{}/{}/", group, app);
                let endpoint_text = endpoint.to_string();
                let setup_url = via_url(&setup_coord, &endpoint_text);
                let html = render_trust_redirect(&setup_url, endpoint, "unknown");
                return PageResponse::html(html);
            }
        }
    }

    // Determine MIME type
    let path = url.split("://").nth(1).unwrap_or("");
    let location_hint = if content_type.is_empty() { path } else { "" };
    let mime = if content_type.is_empty() {
        mime_from_path(if location_hint.is_empty() { path } else { location_hint })
    } else {
        &content_type
    };

    // Resolve site credentials (window.home) and route credentials (window.route)
    let mut response = {
        // Transform markdown to HTML
        if mime == "text/markdown" || path.ends_with(".md") {
            let title = path.rsplit('/').next().unwrap_or("Document");
            match markdown_to_html(&body, title) {
                Ok(html_bytes) => PageResponse::new("text/html", html_bytes).with_packet(packet),
                Err(e) => return PageResponse::error("Markdown Error", &e, None),
            }
        } else {
            PageResponse::new(mime.to_string(), body).with_packet(packet)
        }
    };

    // Set endpoint for window.route
    response.hppr_endpoint = Some(endpoint.to_string());

    // Ensure site credentials for window.home
    if !group.is_empty() && !app.is_empty() {
        if let Ok(site_cred) = credential_store
            .get_or_create_site_credential_async(group, app, client)
            .await
        {
            response.site_credentials = Some((
                site_cred.ring1_name.clone(),
                site_cred.signing_key().to_string(),
            ));
        }
    }

    // Set route signer for window.route (Ring2 identity on remote repos)
    if !is_repo && !group.is_empty() {
        if let Ok(route_cred) = credential_store
            .get_or_create_route_credential_async(group, client)
            .await
        {
            response.hppr_signer = Some(
                format!("@{}#{}",  group, route_cred.signing_key()),
            );
        }
    }

    response
}

/// Handle LIST requests.
async fn handle_list(
    client: &Arc<HpprdClientAsync>,
    _endpoint: &ViaSpec,
    url: &str,
    urc: &str,
) -> PageResponse {
    match client.list(urc).await {
        Ok(children) => {
            let path = url.split("://").nth(1).unwrap_or("");
            let html = render_list_html(path, &children);
            PageResponse::html(html)
        },
        Err(e) => {
            PageResponse::error("HPPR Error", &e, Some(&format!("URL: {}", url)))
        },
    }
}

/// Render redirect page to hppr-setup for direct connections without existing route.
fn render_setup_redirect(setup_url: &str, host: &str, group: &str, app: &str) -> String {
    let css = r#"
        body { max-width: 600px; margin: 80px auto; text-align: center; }
        h1 { color: #f39c12; }
    "#;

    let escaped_url = html_escape(setup_url);
    let escaped_host = html_escape(host);
    let escaped_group = html_escape(group);
    let escaped_app = html_escape(app);

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta http-equiv="refresh" content="0; url={setup_url}">
    <title>Setup Required - HAVI</title>
    <style>{base}{extra}</style>
</head>
<body>
    <h1>Setup Required</h1>
    <p>Connecting to <strong>{host}</strong> for <code>//{group}/{app}/</code></p>
    <p>Redirecting to setup page...</p>
    <p><a href="{setup_url}">Click here if not redirected</a></p>
</body>
</html>"#,
        base = crate::page_shell::BASE_CSS,
        extra = css,
        setup_url = escaped_url,
        host = escaped_host,
        group = escaped_group,
        app = escaped_app,
    )
}

/// Render redirect page to hppr-setup for untrusted seals.
fn render_trust_redirect(setup_url: &str, endpoint: &ViaSpec, trust_key: &str) -> String {
    let key_display = if trust_key.len() > 32 {
        format!("{}...{}", &trust_key[..20], &trust_key[trust_key.len()-8..])
    } else {
        trust_key.to_string()
    };

    let css = r#"
        body { max-width: 600px; margin: 80px auto; text-align: center; }
        h1 { color: #f39c12; }
        .info { color: #888; margin: 20px 0; }
        .key { color: #4ecdc4; font-family: monospace; font-size: 0.9em; }
    "#;

    let escaped_url = html_escape(setup_url);
    let endpoint_text = endpoint.to_string();
    let escaped_endpoint = html_escape(&endpoint_text);
    let escaped_key = html_escape(&key_display);

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Trust Required - HAVI</title>
    <meta http-equiv="refresh" content="0;url={setup_url}">
    <style>{base}{extra}</style>
</head>
<body>
    <h1>Trust Required</h1>
    <p class="info">Content signed by <span class="key">{key_display}</span></p>
    <p>Redirecting to trust setup for <strong>{endpoint}</strong>...</p>
    <p><a href="{setup_url}">Click here if not redirected</a></p>
</body>
</html>"#,
        base = crate::page_shell::BASE_CSS,
        extra = css,
        setup_url = escaped_url,
        endpoint = escaped_endpoint,
        key_display = escaped_key,
    )
}

/// Render directory listing as HTML.
fn render_list_html(path: &str, children: &[String]) -> String {
    let links: String = children
        .iter()
        .map(|child| {
            let encoded = utf8_percent_encode(child, PATH_SEGMENT_ENCODE_SET).to_string();
            format!(r#"<li><a href="./{}">{}</a></li>"#, encoded, html_escape(child))
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

    let escaped_path = html_escape(path);
    let content = if children.is_empty() {
        r#"<p class="empty">(empty)</p>"#.to_string()
    } else {
        format!("<ul>\n            {}\n        </ul>", links)
    };

    let body = format!("    <h1>Index of {}</h1>\n    {}", escaped_path, content);
    crate::page_shell::render_page(&format!("Index of {}", escaped_path), css, &body)
}

/// Render a user-friendly 404 page with coordinate info and editor link.
fn render_not_found_response(url: &str) -> PageResponse {
    let (coordinate, editor_url) = match HAVIAddress::parse(url) {
        Ok(addr) => {
            let parts = addr.parts();
            let location = addr.location_with_slash();
            let coord = if parts.group.is_empty() {
                "//".to_string()
            } else if parts.app.is_empty() {
                format!("//{}/", parts.group)
            } else if location.is_empty() || location == "/" {
                format!("//{}/{}/", parts.group, parts.app)
            } else {
                format!("//{}/{}/{}", parts.group, parts.app, location)
            };
            let editor = format!("hppr-editor://{}/{}/{}", parts.group, parts.app, location);
            (coord, editor)
        },
        Err(_) => {
            let path = url.split("://").nth(1).unwrap_or("");
            (path.to_string(), format!("hppr-editor://{}", path))
        },
    };

    let css = r#"
        body { max-width: 600px; margin: 100px auto; }
        h1 { color: #ff6b6b; }
        .coordinate {
            background: #2d2d4e;
            padding: 16px 20px;
            border-radius: 8px;
            font-family: monospace;
            font-size: 1.1em;
            color: #4ecdc4;
            margin: 20px 0;
        }
        .message { color: #aaa; margin: 16px 0; }
        .actions { margin-top: 24px; }
        .actions a {
            display: inline-block;
            background: #4ecdc4;
            color: #1a1a2e;
            padding: 10px 20px;
            border-radius: 6px;
            text-decoration: none;
            font-weight: 500;
        }
        .actions a:hover { background: #7fdbff; }
    "#;

    let body = format!(
        r#"    <h1>Not Found</h1>
    <p class="message">No packet exists at this coordinate:</p>
    <div class="coordinate">{coordinate}</div>
    <div class="actions">
        <a href="{editor_url}">Create with Editor</a>
    </div>"#,
        coordinate = html_escape(&coordinate),
        editor_url = html_escape(&editor_url),
    );

    PageResponse::html(crate::page_shell::render_page("Not Found", css, &body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hppr_url_routed() {
        let url = HAVIAddress::parse("hppr://chess/games/123").unwrap();
        assert!(url.is_routed());
        assert_eq!(url.group().unwrap(), "chess");
        assert_eq!(url.app().unwrap(), "games");
        assert_eq!(url.location().unwrap(), "123");
    }

    #[test]
    fn test_parse_hppr_url_direct() {
        let url = HAVIAddress::parse("hppr://chess/games/123{via:192.168.1.5:4777}").unwrap();
        assert!(url.is_direct());
        assert_eq!(url.endpoint().unwrap().to_string(), "192.168.1.5:4777");
        assert_eq!(url.group().unwrap(), "chess");
        assert_eq!(url.app().unwrap(), "games");
        assert_eq!(url.location().unwrap(), "123");
    }

    #[test]
    fn test_build_urc_string() {
        assert_eq!(HAVIAddress::build_urc_string("", "", ""), "//");
        assert_eq!(HAVIAddress::build_urc_string("u", "", ""), "//u/");
        assert_eq!(HAVIAddress::build_urc_string("u", "app", ""), "//u/app");
        assert_eq!(HAVIAddress::build_urc_string("u", "app", "loc"), "//u/app/loc");
        assert_eq!(HAVIAddress::build_urc_string("u", "app", "/"), "//u/app/");
        assert_eq!(HAVIAddress::build_urc_string("u", "app", "loc/"), "//u/app/loc/");
    }

    #[test]
    fn test_list_vs_get_detection_with_trailing_slash() {
        let url = HAVIAddress::parse("hppr://group/app/").unwrap();
        assert!(url.is_listing());
        let urc = HAVIAddress::build_urc_string(
            &url.group().unwrap(),
            &url.app().unwrap(),
            "/",
        );
        assert_eq!(urc, "//group/app/");
        assert!(matches!(detect_mode(&urc), HpprMode::List));

        let url = HAVIAddress::parse("hppr://group/app").unwrap();
        assert!(!url.is_listing());
        let urc = HAVIAddress::build_urc_string(
            &url.group().unwrap(),
            &url.app().unwrap(),
            "",
        );
        assert_eq!(urc, "//group/app");
        assert!(matches!(detect_mode(&urc), HpprMode::Get));

        let url = HAVIAddress::parse("hppr://group/app/path/").unwrap();
        assert!(url.is_listing());

        let url = HAVIAddress::parse("hppr://group/app/path").unwrap();
        assert!(!url.is_listing());
    }

    #[test]
    fn test_is_repo_endpoint() {
        let parse = |s: &str| parse_via(s).unwrap();
        assert!(is_repo_endpoint(&parse("127.0.0.1:4777"), &parse("127.0.0.1:4777")));
        assert!(!is_repo_endpoint(&parse("192.168.1.10:4777"), &parse("127.0.0.1:4777")));
    }
}
