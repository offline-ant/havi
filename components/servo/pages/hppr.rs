/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR page handler.
//!
//! This module keeps only page-facing rendering behavior.
//! Shared HPPR source resolution lives in `crate::resolve`.

use std::sync::Arc;

use hppr_client::Signer;
use hppr_packet::urc::UrcMethod;

use crate::PageResponse;
use crate::hppr::client::HpprdClientAsync;
use crate::hppr::credentials::CredentialStoreHandle;
use crate::hppr::resolve::{
    HpprResolveError, resolve_document_with_snapshot, resolve_listing_with_snapshot,
};
use crate::hppr::url::HAVIAddress;
use crate::hppr::util::{html_escape, markdown_to_html, mime_from_path, render_hppr_error_page};

/// Handle an hppr:// URL request.
pub async fn handle_request(
    url: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
    reuse_source: Option<&net_traits::HpprDocumentSourceSnapshot>,
) -> PageResponse {
    log::info!("hppr::handle_request url={}", url);

    let address = match HAVIAddress::parse(url) {
        Ok(u) => u,
        Err(e) => return PageResponse::error("Invalid URL", &e.to_string(), None),
    };

    let parts = address.parts();

    if address.is_listing() {
        handle_list(url, &parts.group, &parts.app, client, credential_store, reuse_source).await
    } else {
        handle_get(url, &parts.group, &parts.app, client, credential_store, reuse_source).await
    }
}

async fn handle_get(
    url: &str,
    group: &str,
    app: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
    reuse_source: Option<&net_traits::HpprDocumentSourceSnapshot>,
) -> PageResponse {
    if (group.is_empty() || app.is_empty())
        && !matches!(HAVIAddress::parse(url).ok().map(|address| address.urc().method()), Some(UrcMethod::Hash))
    {
        return PageResponse::error(
            "HPPR Error",
            "Group and app are required",
            Some(&format!("URL: {}", url)),
        );
    }

    let resolved = match resolve_document_with_snapshot(url, client, credential_store, reuse_source).await {
        Ok(resolved) => resolved,
        Err(error) => {
            if let Ok(address) = HAVIAddress::parse(url)
                && let Some(response) = classify_routed_error(url, group, app, &address, &error)
            {
                return response;
            }
            if error.message.contains("NOT_FOUND") {
                eprintln!("[havi] hppr error: url={} action=not-found error={}", url, error.message);
                return render_not_found_response(url, Some(&error.lookup_trace));
            }
            eprintln!("[havi] hppr error: url={} action=error error={}", url, error.message);
            return PageResponse::html(render_hppr_error_page(
                "HPPR Error",
                &error.message,
                Some(&format!("URL: {}", url)),
                Some(&error.lookup_trace),
            ))
            .with_hppr_lookup_trace(error.lookup_trace);
        },
    };

    let content_type = resolved
        .packet
        .header("Content-Type")
        .unwrap_or("")
        .to_string();
    let context_group = if group.is_empty() {
        resolved.packet.header("Group").unwrap_or("").to_string()
    } else {
        group.to_string()
    };
    let context_app = if app.is_empty() {
        resolved.packet.header("App").unwrap_or("").to_string()
    } else {
        app.to_string()
    };
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
        resolved.content_authority.as_deref(),
        Some(&resolved.hppr_source),
        &context_group,
        &context_app,
        client,
        credential_store,
    )
    .await;
    response.hppr_lookup_trace = Some(resolved.lookup_trace);

    response
}

async fn handle_list(
    url: &str,
    group: &str,
    app: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
    reuse_source: Option<&net_traits::HpprDocumentSourceSnapshot>,
) -> PageResponse {
    match resolve_listing_with_snapshot(url, client, credential_store, reuse_source).await {
        Ok(resolved) => {
            let path = url.split("://").nth(1).unwrap_or("");
            let entries: Vec<(String, bool)> = resolved
                .children
                .iter()
                .map(|child| (child.clone(), child.ends_with('/')))
                .collect();
            let mut response = PageResponse::html(crate::hppr::util::render_directory_listing(path, &entries));
            apply_page_context(
                &mut response,
                &resolved.endpoint.to_string(),
                resolved.signer.as_ref(),
                resolved.content_authority.as_deref(),
                Some(&resolved.hppr_source),
                group,
                app,
                client,
                credential_store,
            )
            .await;
            response.hppr_lookup_trace = Some(resolved.lookup_trace);
            response
        },
        Err(error) => {
            if let Ok(address) = HAVIAddress::parse(url)
                && let Some(response) = classify_routed_error(url, group, app, &address, &error)
            {
                return response;
            }
            eprintln!("[havi] hppr list error: url={} action=error error={}", url, error.message);
            PageResponse::html(render_hppr_error_page(
                "HPPR Error",
                &error.message,
                Some(&format!("URL: {}", url)),
                Some(&error.lookup_trace),
            ))
            .with_hppr_lookup_trace(error.lookup_trace)
        },
    }
}

async fn apply_page_context(
    response: &mut PageResponse,
    _endpoint: &str,
    _signer: Option<&Signer>,
    content_authority: Option<&str>,
    hppr_source: Option<&net_traits::HpprDocumentSource>,
    group: &str,
    app: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) {
    let _ = (group, app, client, credential_store);

    // Ordinary hppr:// documents now carry their committed source through
    // hppr_source directly. Site/home credentials are no longer injected into
    // ordinary document metadata. Helper pages keep any explicit privileged
    // access inside helper page code instead of document transport metadata.
    response.hppr_content_authority = content_authority.map(str::to_string);
    response.hppr_source = hppr_source.cloned();
}

fn classify_routed_error(
    url: &str,
    _group: &str,
    _app: &str,
    address: &HAVIAddress,
    error: &HpprResolveError,
) -> Option<PageResponse> {
    if !address.has_direct_endpoint() && !address.is_routed() {
        return None;
    }

    if error.message.contains("UNAUTHORIZED not a member") {
        eprintln!(
            "[havi] hppr error: url={} action=membership-required error={}",
            url, error.message
        );
        return Some(
            PageResponse::html(render_hppr_error_page(
                "Membership Required",
                &error.message,
                Some("Automatic join helper pages were removed. Use an explicit external join workflow."),
                Some(&error.lookup_trace),
            ))
            .with_hppr_lookup_trace(error.lookup_trace.clone()),
        );
    }

    if error.message.contains("NOT_FOUND ring2 setup") {
        eprintln!(
            "[havi] hppr error: url={} action=ring2-setup-missing error={}",
            url, error.message
        );
        return Some(
            PageResponse::html(render_hppr_error_page(
                "Route Setup Error",
                &error.message,
                Some("Target repo is missing Ring2 setup for this group."),
                Some(&error.lookup_trace),
            ))
            .with_hppr_lookup_trace(error.lookup_trace.clone()),
        );
    }

    if error.message.contains("MEMBERS resolution failed") {
        eprintln!(
            "[havi] hppr error: url={} action=route-membership-error error={}",
            url, error.message
        );
        return Some(
            PageResponse::html(render_hppr_error_page(
                "Route Membership Error",
                &error.message,
                Some("Target repo membership configuration is broken or incomplete."),
                Some(&error.lookup_trace),
            ))
            .with_hppr_lookup_trace(error.lookup_trace.clone()),
        );
    }

    if error.message.contains("UNAUTHORIZED") {
        eprintln!("[havi] hppr error: url={} action=routed-auth-error error={}", url, error.message);
        return Some(
            PageResponse::html(render_hppr_error_page(
                "Routed Auth Error",
                &error.message,
                Some(&format!("URL: {}", url)),
                Some(&error.lookup_trace),
            ))
            .with_hppr_lookup_trace(error.lookup_trace.clone()),
        );
    }

    None
}

/// Render a user-friendly 404 page with coordinate info.
fn render_not_found_response(
    url: &str,
    lookup_trace: Option<&embedder_traits::HpprLookupTrace>,
) -> PageResponse {
    let coordinate = match HAVIAddress::parse(url) {
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
            coord
        },
        Err(_) => url.split("://").nth(1).unwrap_or("").to_string(),
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
    "#;

    let details_html = lookup_trace
        .map(|trace| {
            format!(
                r#"<details><summary>Lookup details</summary><pre>{}</pre></details>"#,
                html_escape(&trace.format_text())
            )
        })
        .unwrap_or_default();

    let body = format!(
        r#"    <h1>Not Found</h1>
    <p class="message">No packet exists at this coordinate:</p>
    <div class="coordinate">{coordinate}</div>
    {details_html}"#,
        coordinate = html_escape(&coordinate),
        details_html = details_html,
    );

    let mut response = PageResponse::html(crate::pages::page_shell::render_page("Not Found", css, &body));
    if let Some(lookup_trace) = lookup_trace.cloned() {
        response.hppr_lookup_trace = Some(lookup_trace);
    }
    response
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
