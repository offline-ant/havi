/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR Browse page handler.
//!
//! Handles hppr-browse:// URLs for read-only directory browsing.
//! URL format: hppr-browse://group/app/path/
//!
//! Always uses LIST mode. Shows directory entries with:
//! - Click on entry name -> stays in browse mode (hppr-browse://)
//! - "open" link -> switches to content mode (hppr://)

use std::sync::Arc;

use percent_encoding::utf8_percent_encode;

use crate::PageResponse;
use crate::hppr::client::HpprdClientAsync;
use crate::hppr::credentials::CredentialStoreHandle;
use crate::hppr::resolve::resolve_listing;
use crate::hppr::url::{HAVIAddress, via_url};
use crate::hppr::util::{PATH_SEGMENT_ENCODE_SET, html_escape};



/// Handle an hppr-browse:// URL request.
pub async fn handle_request(
    url: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    let address = match HAVIAddress::parse(url) {
        Ok(u) => u,
        Err(e) => {
            return PageResponse::error("Browse Error", &e.to_string(), None);
        },
    };

    let parts = address.parts();
    let mut location = parts.location.clone();
    if !location.ends_with('/') {
        location.push('/');
    }

    match resolve_listing(url, client, credential_store).await {
        Ok(listing) => {
            let endpoint_str = if let Some(ep) = address.endpoint_string() {
                Some(ep)
            } else if listing.is_repo {
                None
            } else {
                Some(listing.endpoint.to_string())
            };
            let html = render_browse_html(
                &parts.group,
                &parts.app,
                &location,
                &listing.children,
                endpoint_str.as_deref(),
            );
            PageResponse::html(html)
        }
        Err(e) => PageResponse::error("Browse Error", &e, None),
    }
}

/// Build breadcrumb navigation.
fn render_breadcrumb(group: &str, app: &str, location: &str, endpoint: Option<&str>) -> String {
    let mut crumbs = Vec::new();

    let browse_url = |coord: &str| match endpoint {
        Some(ep) => via_url(coord, ep),
        None => coord.to_string(),
    };

    if !group.is_empty() {
        let url = browse_url(&format!("hppr-browse://{}/", group));
        crumbs.push(format!(
            r#"<a href="{}">{}</a>"#,
            html_escape(&url),
            html_escape(group)
        ));
    }

    if !app.is_empty() {
        let url = browse_url(&format!("hppr-browse://{}/{}/", group, app));
        crumbs.push(format!(
            r#"<a href="{}">{}</a>"#,
            html_escape(&url),
            html_escape(app)
        ));
    }

    let loc_parts: Vec<&str> = location
        .trim_end_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    let mut path_so_far = String::new();
    for (i, part) in loc_parts.iter().enumerate() {
        if !path_so_far.is_empty() {
            path_so_far.push('/');
        }
        path_so_far.push_str(part);

        let is_last = i == loc_parts.len() - 1;
        if is_last {
            crumbs.push(html_escape(part));
        } else {
            let url = browse_url(&format!("hppr-browse://{}/{}/{}/", group, app, path_so_far));
            crumbs.push(format!(
                r#"<a href="{}">{}</a>"#,
                html_escape(&url),
                html_escape(part)
            ));
        }
    }

    crumbs.join(" / ")
}

/// Render directory listing as HTML with browse and open links.
fn render_browse_html(
    group: &str,
    app: &str,
    location: &str,
    children: &[String],
    endpoint: Option<&str>,
) -> String {
    let display_urc = HAVIAddress::build_urc_string(group, app, location);
    let breadcrumb = render_breadcrumb(group, app, location, endpoint);

    let hppr_url = |coord: &str| match endpoint {
        Some(ep) => via_url(coord, ep),
        None => coord.to_string(),
    };

    let base_path = if location.is_empty() || location == "/" {
        format!("//{}/{}", group, app)
    } else {
        let loc_trimmed = location.trim_end_matches('/');
        format!("//{}/{}/{}", group, app, loc_trimmed)
    };

    let entries: String = children
        .iter()
        .map(|child| {
            let encoded = utf8_percent_encode(child, PATH_SEGMENT_ENCODE_SET).to_string();
            let child_escaped = html_escape(child);

            let is_dir = child.ends_with('/');

            if is_dir {
                let child_name = child.trim_end_matches('/');
                let hppr_link = hppr_url(&format!("hppr:{}/{}/", base_path, child_name));
                format!(
                    r#"<li>
            <a class="entry-name" href="./{encoded}">{child_escaped}</a>
            <a class="open-hppr" href="{hppr_url}">open</a>
        </li>"#,
                    encoded = encoded,
                    child_escaped = child_escaped,
                    hppr_url = html_escape(&hppr_link),
                )
            } else {
                let hppr_link = hppr_url(&format!("hppr:{}/{}", base_path, child));
                format!(
                    r#"<li>
            <a class="entry-name" href="./{encoded}/">{child_escaped}</a>
            <a class="open-hppr" href="{hppr_url}">open</a>
        </li>"#,
                    encoded = encoded,
                    child_escaped = child_escaped,
                    hppr_url = html_escape(&hppr_link),
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n        ");

    let content = if children.is_empty() {
        r#"<p class="empty">(empty)</p>"#.to_string()
    } else {
        format!("<ul>\n        {}\n    </ul>", entries)
    };

    let css = r#"
        body { max-width: 800px; margin: 40px auto; }
        h1 { border-bottom: 2px solid #4ecdc4; padding-bottom: 10px; }
        .breadcrumb { margin-bottom: 20px; }
        ul { list-style: none; padding: 0; }
        li {
            padding: 8px 0;
            border-bottom: 1px solid #333;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }
        .entry-name { font-family: monospace; }
        .entry-name:hover { color: #fff; }
        .open-hppr {
            color: #4ecdc4;
            text-decoration: none;
            padding: 4px 8px;
            border: 1px solid #4ecdc4;
            border-radius: 4px;
            font-size: 0.9em;
        }
        .open-hppr:hover { background: #4ecdc4; color: #1a1a2e; }
        .empty { color: #888; font-style: italic; }
    "#;

    let escaped_urc = html_escape(&display_urc);
    let body = format!(
        "    <div class=\"breadcrumb\">{breadcrumb}</div>\n    <h1>Browse {urc}</h1>\n    {content}",
        breadcrumb = breadcrumb,
        urc = escaped_urc,
        content = content,
    );

    crate::pages::page_shell::render_page(&format!("Browse {}", escaped_urc), css, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_browse_url_via_hppr_url() {
        let url = HAVIAddress::parse("hppr-browse://mygroup/myapp/some/path").unwrap();
        assert_eq!(url.group(), Some("mygroup".to_string()));
        assert_eq!(url.app(), Some("myapp".to_string()));
        assert_eq!(url.location(), Some("some/path".to_string()));

        let url = HAVIAddress::parse("hppr-browse://repo/admin/").unwrap();
        assert_eq!(url.group(), Some("repo".to_string()));
        assert_eq!(url.app(), Some("admin".to_string()));
        assert!(url.is_listing());
    }

    #[test]
    fn test_build_urc() {
        assert_eq!(HAVIAddress::build_urc_string("", "", ""), "//");
        assert_eq!(HAVIAddress::build_urc_string("g", "", ""), "//g/");
        assert_eq!(HAVIAddress::build_urc_string("g", "a", "/"), "//g/a/");
        assert_eq!(HAVIAddress::build_urc_string("g", "a", "path/"), "//g/a/path/");
    }

    #[test]
    fn test_render_breadcrumb() {
        let bc = render_breadcrumb("repo", "admin", "users/", None);
        assert!(bc.contains("repo"));
        assert!(bc.contains("admin"));
        assert!(bc.contains("users"));
        assert!(bc.contains("hppr-browse://"));
    }

    #[test]
    fn test_render_breadcrumb_with_endpoint() {
        let bc = render_breadcrumb("repo", "admin", "users/", Some("192.168.1.5:4777"));
        assert!(bc.contains("repo"));
        assert!(bc.contains("admin"));
        assert!(bc.contains("users"));
        assert!(bc.contains("hppr-browse://repo/{via:192.168.1.5:4777}"));
    }

    #[test]
    fn test_parse_browse_url_with_endpoint() {
        let url = HAVIAddress::parse("hppr-browse://mygroup/myapp/path/{via:192.168.1.5}").unwrap();
        let ep = url.endpoint().unwrap();
        assert_eq!(ep.host(), "192.168.1.5");
        assert_eq!(ep.port(), 4777);
        assert_eq!(url.group(), Some("mygroup".to_string()));
        assert_eq!(url.app(), Some("myapp".to_string()));
    }
}
