/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared page shell for protocol handler HTML pages.
//!
//! Provides the common dark-themed HTML boilerplate used across all
//! protocol handlers. Each handler adds its own page-specific CSS.

/// Base CSS reset and dark theme used by all generated pages.
pub const BASE_CSS: &str = r#"
* { box-sizing: border-box; }
body {
    font-family: system-ui, sans-serif;
    margin: 0;
    padding: 20px;
    background: #1a1a2e;
    color: #eee;
    min-height: 100vh;
}
a { color: #7fdbff; text-decoration: none; }
a:hover { text-decoration: underline; }
h1, h2, h3 { color: #4ecdc4; }
"#;

/// Wrap body content in a complete HTML page with the shared dark theme.
///
/// `extra_css` is page-specific CSS appended after BASE_CSS.
/// `body` is the inner HTML of `<body>`.
pub fn render_page(title: &str, extra_css: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>{title}</title>
    <style>{base}{extra}</style>
</head>
<body>
{body}
</body>
</html>"#,
        title = title,
        base = BASE_CSS,
        extra = extra_css,
        body = body,
    )
}
