/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR page handler.
//!
//! Handles hppr:// URLs by fetching content from an HPPR daemon.
//! URL format: hppr://group/app/location
//! LIST mode: hppr://group/app/location/ (trailing slash)

use std::sync::Arc;

use hppr_client::{ViaSpec, parse_via};
use hppr_packet::chunk::{ChunkKind, is_chunk_manifest, parse_chunk_manifest};
use crate::PageResponse;
use crate::client::HpprdClientAsync;
use crate::credentials::CredentialStoreHandle;
use crate::url::{HAVIAddress, via_url};
use crate::util::{
    RouteEndpointSource, append_location, html_escape, markdown_to_html, mime_from_path,
    resolve_route_endpoint,
};

/// Resolve a HAVIAddress to endpoint and route metadata.
async fn resolve_target(
    url: &HAVIAddress,
    repo_client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
    page_endpoint: Option<&ViaSpec>,
) -> Result<(ViaSpec, Option<String>, RouteEndpointSource), String> {
    let repo_target = repo_client.target();

    let parts = url.parts();
    let (endpoint, upstream_key, source) = if let Some(endpoint) = url.endpoint_string() {
        if endpoint == "repo" {
            (repo_target, None, RouteEndpointSource::HomeFallback)
        } else {
            let via = parse_via(&endpoint).map_err(|e| e.to_string())?;
            (via, None, RouteEndpointSource::Routed)
        }
    } else if let Some(endpoint) = page_endpoint {
        (endpoint.clone(), None, RouteEndpointSource::Routed)
    } else {
        resolve_route_endpoint(&parts.group, &parts.app, repo_client, credential_store).await
    };

    Ok((endpoint, upstream_key, source))
}

async fn resolve_deployment_target(
    route_client: &Arc<HpprdClientAsync>,
    group: &str,
    app: &str,
    requested_location: &str,
    upstream_key: Option<&str>,
    is_listing: bool,
) -> Result<String, String> {
    let repo_vkey = match upstream_key {
        Some(key) => key.to_string(),
        None => route_client.get_admin_identity().await?,
    };

    let deploy = route_client
        .get_deploy(group, app, &repo_vkey)
        .await?;

    let target = append_location(&deploy.root, requested_location);
    Ok(if is_listing {
        format!("{}/", target.trim_end_matches('/'))
    } else {
        format!("{}/|/seal/{}", target, deploy.signer)
    })
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
                Some(_) => match client.get_admin_identity().await {
                    Ok(key) => client
                        .get_route(&parts.group, &parts.app, &key)
                        .await
                        .is_ok(),
                    Err(_) => false,
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
                let redirect_html =
                    render_setup_redirect(&setup_url, &host, &parts.group, &parts.app);
                return PageResponse::html(redirect_html);
            }
        }
    }

    let (endpoint, upstream_key, source) = match resolve_target(&address, client, credential_store, None).await {
        Ok(r) => r,
        Err(e) => {
            return PageResponse::error("HPPR Error", &e, Some(&format!("URL: {}", url)));
        },
    };

    let urc = HAVIAddress::build_urc_string(&parts.group, &parts.app, &location);
    log::info!(
        "hppr::handle_request resolved endpoint={} urc={}",
        endpoint,
        urc
    );
    let is_repo = matches!(source, RouteEndpointSource::HomeFallback);
    let is_listing = address.is_listing();

    if is_listing {
        handle_list(
            client,
            &address,
            &endpoint,
            url,
            &urc,
            &parts.group,
            &parts.app,
            is_repo,
            upstream_key.as_deref(),
            credential_store,
        )
        .await
    } else {
        handle_get(
            &address,
            &endpoint,
            url,
            &urc,
            &parts.group,
            &parts.app,
            is_repo,
            upstream_key.as_deref(),
            client,
            credential_store,
        )
        .await
    }
}

/// Reassemble chunk data from a chunk manifest by fetching each chunk blob by hash.
async fn reassemble_chunks(
    client: &Arc<HpprdClientAsync>,
    manifest: &hppr_packet::chunk::ChunkManifest,
) -> Result<Vec<u8>, String> {
    let mut result = Vec::with_capacity(manifest.total_length as usize);

    for chunk in &manifest.chunks {
        let hash_urc = format!("////{}", chunk.hash);
        let blob_packet = client
            .get_packet_authenticated(&hash_urc)
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
                Box::pin(reassemble_chunks(client, &sub_manifest)).await?
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

/// Build a route client for non-repo endpoints (Ring2 signer).
async fn build_route_client(
    endpoint: &ViaSpec,
    group: &str,
    url: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> Result<Arc<HpprdClientAsync>, PageResponse> {
    match credential_store
        .get_or_create_route_credential_async(group, client)
        .await
    {
        Ok(route_cred) => {
            let signer = hppr_client::Signer::ring2(group, route_cred.signing_key());
            Ok(Arc::new(HpprdClientAsync::new_with_signer(
                endpoint.clone(),
                signer,
            )))
        },
        Err(e) => Err(PageResponse::error(
            "HPPR Error",
            &format!("No route credential for {}: {}", group, e),
            Some(&format!("URL: {}", url)),
        )),
    }
}

/// Redirect to hppr-join:// for unauthorized Ring2 access.
fn unauthorized_join_redirect(group: &str, app: &str) -> PageResponse {
    let join_url = format!("hppr-join://{}/{}/", group, app);
    let html = render_join_redirect(&join_url, group, app);
    PageResponse::html(html)
}

/// Handle GET requests.
async fn handle_get(
    address: &HAVIAddress,
    endpoint: &ViaSpec,
    url: &str,
    urc: &str,
    group: &str,
    app: &str,
    is_repo: bool,
    upstream_key: Option<&str>,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    log::info!(
        "handle_get: urc={} endpoint={} is_repo={}",
        urc,
        endpoint,
        is_repo
    );

    if group.is_empty() || app.is_empty() {
        return PageResponse::error(
            "HPPR Error",
            "Group and app are required",
            Some(&format!("URL: {}", url)),
        );
    }

    // Build the appropriate client for the target endpoint.
    // For routed (non-repo) content, connect to the remote with Ring2 signer.
    let route_client: Option<Arc<HpprdClientAsync>> = if !is_repo {
        match build_route_client(endpoint, group, url, client, credential_store).await {
            Ok(c) => Some(c),
            Err(resp) => return resp,
        }
    } else {
        None
    };
    let fetch_client = route_client.as_ref().unwrap_or(client);

    let requested_location = address.location_with_slash();
    let fetch_urc = if is_repo {
        urc.to_string()
    } else {
        match resolve_deployment_target(
            route_client.as_ref().expect("route_client exists for non-repo"),
            group,
            app,
            &requested_location,
            upstream_key,
            false,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                return PageResponse::error("HPPR Error", &e, Some(&format!("URL: {}", url)));
            },
        }
    };

    // Fetch packet via GET
    let fetch_result = fetch_client.get_packet_authenticated(&fetch_urc).await;

    let packet = match fetch_result {
        Ok(p) => p,
        Err(e) => {
            if e.contains("UNAUTHORIZED") && !is_repo {
                return unauthorized_join_redirect(group, app);
            }
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

        match reassemble_chunks(client, &manifest).await {
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
        let ct = packet.header("Content-Type").unwrap_or("").to_string();
        (ct, packet.data().to_vec())
    };

    // Determine MIME type
    let path = url.split("://").nth(1).unwrap_or("");
    let mime = response_mime(&content_type, path);

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
            response.hppr_signer = Some(format!("ring2:{}#{}", group, route_cred.signing_key()));
        }
    }

    response
}

/// Handle LIST requests.
async fn handle_list(
    client: &Arc<HpprdClientAsync>,
    address: &HAVIAddress,
    endpoint: &ViaSpec,
    url: &str,
    urc: &str,
    group: &str,
    app: &str,
    is_repo: bool,
    upstream_key: Option<&str>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    let route_client: Option<Arc<HpprdClientAsync>> = if !is_repo {
        match build_route_client(endpoint, group, url, client, credential_store).await {
            Ok(c) => Some(c),
            Err(resp) => return resp,
        }
    } else {
        None
    };

    let list_client = route_client.as_ref().unwrap_or(client);
    let requested_location = address.location_with_slash();
    let list_urc = if is_repo {
        urc.to_string()
    } else {
        match resolve_deployment_target(
            route_client.as_ref().expect("route_client exists for non-repo"),
            group,
            app,
            &requested_location,
            upstream_key,
            true,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                return PageResponse::error("HPPR Error", &e, Some(&format!("URL: {}", url)));
            },
        }
    };

    match list_client.list(&list_urc).await {
        Ok(children) => {
            let path = url.split("://").nth(1).unwrap_or("");
            let entries: Vec<(String, bool)> = children
                .iter()
                .map(|c| (c.clone(), c.ends_with('/')))
                .collect();
            PageResponse::html(crate::util::render_directory_listing(path, &entries))
        },
        Err(e) => {
            if e.contains("UNAUTHORIZED") && !is_repo {
                return unauthorized_join_redirect(group, app);
            }
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

/// Render redirect page to hppr-join for unauthorized Ring2 access.
fn render_join_redirect(join_url: &str, group: &str, app: &str) -> String {
    let escaped_url = html_escape(join_url);
    let escaped_group = html_escape(group);
    let escaped_app = html_escape(app);

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Join Required - HAVI</title>
    <meta http-equiv="refresh" content="0;url={join_url}">
    <style>{base}</style>
</head>
<body>
    <h1>Join Required</h1>
    <p>Access to <code>//{group}/{app}/</code> requires group membership.</p>
    <p>Redirecting to join page...</p>
    <p><a href="{join_url}">Click here if not redirected</a></p>
</body>
</html>"#,
        base = crate::page_shell::BASE_CSS,
        join_url = escaped_url,
        group = escaped_group,
        app = escaped_app,
    )
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

fn response_mime<'a>(content_type: &'a str, path: &'a str) -> &'a str {
    if content_type.is_empty() {
        mime_from_path(path)
    } else {
        content_type
    }
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
        assert_eq!(
            HAVIAddress::build_urc_string("u", "app", "loc"),
            "//u/app/loc"
        );
        assert_eq!(HAVIAddress::build_urc_string("u", "app", "/"), "//u/app/");
        assert_eq!(
            HAVIAddress::build_urc_string("u", "app", "loc/"),
            "//u/app/loc/"
        );
    }

    #[test]
    fn test_list_vs_get_detection_with_trailing_slash() {
        let url = HAVIAddress::parse("hppr://group/app/").unwrap();
        assert!(url.is_listing());
        let urc = HAVIAddress::build_urc_string(&url.group().unwrap(), &url.app().unwrap(), "/");
        assert_eq!(urc, "//group/app/");

        let url = HAVIAddress::parse("hppr://group/app").unwrap();
        assert!(!url.is_listing());
        let urc = HAVIAddress::build_urc_string(&url.group().unwrap(), &url.app().unwrap(), "");
        assert_eq!(urc, "//group/app");

        let url = HAVIAddress::parse("hppr://group/app/path/").unwrap();
        assert!(url.is_listing());

        let url = HAVIAddress::parse("hppr://group/app/path").unwrap();
        assert!(!url.is_listing());
    }

    #[test]
    fn test_response_mime_uses_path_when_content_type_missing() {
        assert_eq!(response_mime("", "dev/hppr.forge/presentation/dist/reveal.css"), "text/css");
        assert_eq!(response_mime("", "dev/hppr.forge/presentation/dist/reveal.js"), "application/javascript");
        assert_eq!(response_mime("text/html", "dev/hppr.forge/presentation/index.html"), "text/html");
    }
}
