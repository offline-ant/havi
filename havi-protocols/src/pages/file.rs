/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Local file:// page handler.
//!
//! Serves local filesystem content as HPPR HTML pages with site credentials.

use std::sync::Arc;

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use pulldown_cmark::{Options, Parser, html};

use crate::PageResponse;
use crate::client::HpprdClientAsync;
use crate::credentials::CredentialStoreHandle;
use crate::util::{html_escape, mime_from_path};

/// Characters that need encoding in URL path segments.
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

/// Handle a file:// URL request.
pub async fn handle_request(
    url: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> PageResponse {
    log::info!("file::handle_request url={}", url);

    let path = match parse_file_path(url) {
        Ok(p) => p,
        Err(e) => return PageResponse::error("File Error", &e, None),
    };

    let metadata = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(e) => {
            return PageResponse::error(
                "File Not Found",
                &format!("{}: {}", path, e),
                None,
            );
        },
    };

    let mut response = if metadata.is_dir() {
        render_directory(&path)
    } else {
        render_file(&path)
    };

    // Set site credentials: group="file", app="local"
    if let Ok(site_cred) = credential_store
        .get_or_create_site_credential_async("file", "local", client)
        .await
    {
        response.site_credentials = Some((
            site_cred.ring1_name.clone(),
            site_cred.signing_key().to_string(),
        ));
    }

    response
}

/// Parse a file path from a file:// URL, handling percent-decoding.
fn parse_file_path(url: &str) -> Result<String, String> {
    let raw = url
        .strip_prefix("file://")
        .ok_or_else(|| format!("not a file:// URL: {}", url))?;

    // file:///path/to/file → /path/to/file
    // file://localhost/path → /path
    let path_part = if let Some(rest) = raw.strip_prefix("localhost") {
        rest
    } else {
        raw
    };

    let decoded = percent_encoding::percent_decode_str(path_part)
        .decode_utf8()
        .map_err(|e| format!("invalid UTF-8 in path: {}", e))?;

    if decoded.is_empty() {
        return Ok("/".to_string());
    }

    Ok(decoded.into_owned())
}

/// Render a file's content as a PageResponse.
fn render_file(path: &str) -> PageResponse {
    let body = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            return PageResponse::error("Read Error", &format!("{}: {}", path, e), None);
        },
    };

    let mime = mime_from_path(path);

    // Convert markdown to HTML
    if mime == "text/markdown" || path.ends_with(".md") {
        let title = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Document");
        match markdown_to_html(&body, title) {
            Ok(html_bytes) => return PageResponse::new("text/html", html_bytes),
            Err(e) => return PageResponse::error("Markdown Error", &e, None),
        }
    }

    PageResponse::new(mime, body)
}

/// Render a directory listing as HTML.
fn render_directory(path: &str) -> PageResponse {
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(e) => {
            return PageResponse::error("Directory Error", &format!("{}: {}", path, e), None);
        },
    };

    let mut names: Vec<(String, bool)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        names.push((name, is_dir));
    }
    names.sort_by(|a, b| a.0.cmp(&b.0));

    let links: String = names
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

    let escaped_path = html_escape(path);
    let content = if names.is_empty() {
        r#"<p class="empty">(empty)</p>"#.to_string()
    } else {
        format!("<ul>\n            {}\n        </ul>", links)
    };

    let body = format!("    <h1>Index of {}</h1>\n    {}", escaped_path, content);
    PageResponse::html(crate::page_shell::render_page(
        &format!("Index of {}", escaped_path),
        css,
        &body,
    ))
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
