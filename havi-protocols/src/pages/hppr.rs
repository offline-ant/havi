/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR page handler.
//!
//! This module keeps only page-facing rendering behavior.
//! Shared HPPR source resolution lives in `crate::resolve`.

use std::sync::Arc;

use hppr_client::Signer;

use crate::PageResponse;
use crate::client::HpprdClientAsync;
use crate::credentials::CredentialStoreHandle;
use crate::resolve::{
    resolve_document, resolve_listing, route_configured_for_direct_endpoint,
};
use crate::url::{HAVIAddress, via_url};
use crate::util::{html_escape, markdown_to_html, mime_from_path};

/// Handle an hppr:// URL request.
pub async fn handle_request(
    url: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    log::info!("hppr::handle_request url={}", url);

    let address = match HAVIAddress::parse(url) {
        Ok(u) => u,
        Err(e) => return PageResponse::error("Invalid URL", &e.to_string(), None),
    };

    let parts = address.parts();
    let location = address.location_with_slash();

    if let Some(host) = address.endpoint_string() {
        if host != "repo" && !parts.group.is_empty() && !parts.app.is_empty() {
            let route_exists = route_configured_for_direct_endpoint(
                &parts.group,
                &parts.app,
                client,
                credential_store,
            )
            .await;
            if !route_exists {
                let setup_coord = if location.is_empty() || location == "/" {
                    format!("hppr-setup://{}/{}/", parts.group, parts.app)
                } else {
                    format!("hppr-setup://{}/{}/{}", parts.group, parts.app, location)
                };
                let setup_url = via_url(&setup_coord, &host);
                return PageResponse::html(render_setup_redirect(
                    &setup_url,
                    &host,
                    &parts.group,
                    &parts.app,
                ));
            }
        }
    }

    if address.is_listing() {
        handle_list(url, &parts.group, &parts.app, client, credential_store).await
    } else {
        handle_get(url, &parts.group, &parts.app, client, credential_store).await
    }
}

async fn handle_get(
    url: &str,
    group: &str,
    app: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    if group.is_empty() || app.is_empty() {
        return PageResponse::error(
            "HPPR Error",
            "Group and app are required",
            Some(&format!("URL: {}", url)),
        );
    }

    let resolved = match resolve_document(url, client, credential_store).await {
        Ok(resolved) => resolved,
        Err(error) => {
            if let Ok(address) = HAVIAddress::parse(url)
                && let Some(response) = classify_routed_error(url, group, app, &address, &error)
            {
                return response;
            }
            if error.contains("NOT_FOUND") {
                eprintln!("[havi] hppr error: url={} action=not-found error={}", url, error);
                return render_not_found_response(url);
            }
            eprintln!("[havi] hppr error: url={} action=error error={}", url, error);
            return PageResponse::error("HPPR Error", &error, Some(&format!("URL: {}", url)));
        },
    };

    let content_type = resolved
        .packet
        .header("Content-Type")
        .unwrap_or("")
        .to_string();
    let path = url.split("://").nth(1).unwrap_or("");
    let mime = response_mime(&content_type, path);

    let mut response = if mime == "text/markdown" || path.ends_with(".md") {
        let title = path.rsplit('/').next().unwrap_or("Document");
        match markdown_to_html(resolved.packet.data(), title) {
            Ok(html_bytes) => PageResponse::new("text/html", html_bytes).with_packet(resolved.packet),
            Err(error) => return PageResponse::error("Markdown Error", &error, None),
        }
    } else {
        PageResponse::new(mime.to_string(), resolved.packet.data().to_vec()).with_packet(resolved.packet)
    };

    apply_page_context(
        &mut response,
        &resolved.endpoint.to_string(),
        resolved.signer.as_ref(),
        resolved.content_signer.as_deref(),
        group,
        app,
        client,
        credential_store,
    )
    .await;

    response
}

async fn handle_list(
    url: &str,
    group: &str,
    app: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    match resolve_listing(url, client, credential_store).await {
        Ok(resolved) => {
            let path = url.split("://").nth(1).unwrap_or("");
            let entries: Vec<(String, bool)> = resolved
                .children
                .iter()
                .map(|child| (child.clone(), child.ends_with('/')))
                .collect();
            let mut response = PageResponse::html(crate::util::render_directory_listing(path, &entries));
            apply_page_context(
                &mut response,
                &resolved.endpoint.to_string(),
                resolved.signer.as_ref(),
                resolved.content_signer.as_deref(),
                group,
                app,
                client,
                credential_store,
            )
            .await;
            response
        },
        Err(error) => {
            if let Ok(address) = HAVIAddress::parse(url)
                && let Some(response) = classify_routed_error(url, group, app, &address, &error)
            {
                return response;
            }
            eprintln!("[havi] hppr list error: url={} action=error error={}", url, error);
            PageResponse::error("HPPR Error", &error, Some(&format!("URL: {}", url)))
        },
    }
}

async fn apply_page_context(
    response: &mut PageResponse,
    endpoint: &str,
    signer: Option<&Signer>,
    content_signer: Option<&str>,
    group: &str,
    app: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) {
    response.hppr_endpoint = Some(endpoint.to_string());

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

    response.hppr_signer = signer.and_then(signer_identity_string);
    response.hppr_content_signer = content_signer.map(str::to_string);
}

fn signer_identity_string(signer: &Signer) -> Option<String> {
    match signer {
        Signer::Ring2 { group, signing_key } => Some(format!("ring2:{}#{}", group, signing_key)),
        Signer::Ring1 {
            ring1_name,
            signing_key,
        } => Some(format!("ring1:{}#{}", ring1_name, signing_key)),
        Signer::Ring1Adhoc { token, ring1_name } => {
            Some(format!("ring1:{}#{}", ring1_name, token))
        },
        Signer::Ring2Adhoc {
            credential_input, ..
        } => Some(format!("ring2:{}", credential_input)),
        Signer::Ring2Contextual { username, password } => {
            Some(format!("ring2:/{}#{}", username, password))
        },
        Signer::Anyone { .. } => None,
    }
}

fn classify_routed_error(
    url: &str,
    group: &str,
    app: &str,
    address: &HAVIAddress,
    error: &str,
) -> Option<PageResponse> {
    if !address.has_direct_endpoint() && !address.is_routed() {
        return None;
    }

    if error.contains("UNAUTHORIZED not a member") {
        eprintln!("[havi] hppr error: url={} action=join error={}", url, error);
        return Some(unauthorized_join_redirect(group, app));
    }

    if error.contains("NOT_FOUND ring2 setup") {
        eprintln!("[havi] hppr error: url={} action=ring2-setup-missing error={}", url, error);
        return Some(PageResponse::error(
            "Route Setup Error",
            error,
            Some("Target repo is missing Ring2 setup for this group."),
        ));
    }

    if error.contains("MEMBERS resolution failed") {
        eprintln!("[havi] hppr error: url={} action=route-membership-error error={}", url, error);
        return Some(PageResponse::error(
            "Route Membership Error",
            error,
            Some("Target repo membership configuration is broken or incomplete."),
        ));
    }

    if error.contains("UNAUTHORIZED") {
        eprintln!("[havi] hppr error: url={} action=routed-auth-error error={}", url, error);
    }

    None
}

fn unauthorized_join_redirect(group: &str, app: &str) -> PageResponse {
    let join_url = format!("hppr-join://{}/{}/", group, app);
    PageResponse::html(render_join_redirect(&join_url, group, app))
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
