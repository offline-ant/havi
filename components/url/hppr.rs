/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR URL types for HAVI protocol handlers.
//!
//! Provides unified parsing for all HPPR-family URLs:
//! - `hppr://`, `hppr-setup:`, `hppr-sandbox:`, `hppr-browse://`, `hppr-editor://` - URC-based URLs
//! - `havi://` - Admin page URLs with simple path format
//!
//! Endpoint is specified via `{via:host:port}` JSONqa suffix, not as a prefix.
//! Example: `hppr://chess/game/board.html{via:192.168.1.5:4777}`

use hppr_packet::urc::URC;
use hppr_packet::CoordinateParts;
use std::fmt;

/// HPPR URL schemes that use URCs.
///
/// Note: `havi://` is not included as it doesn't use URCs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HpprScheme {
    /// hppr:// - Standard content retrieval with optional route lookup
    Hppr,
    /// hppr-setup: - Trust establishment (requires endpoint via {via:...})
    HpprSetup,
    /// hppr-sandbox: - Sandboxed preview (requires endpoint via {via:...})
    HpprSandbox,
    /// hppr-browse:// - Directory browser
    HpprBrowse,
    /// hppr-editor:// - Local editor (no endpoint)
    HpprEditor,
}

impl HpprScheme {
    /// Returns the scheme prefix string.
    pub fn prefix(&self) -> &'static str {
        match self {
            HpprScheme::Hppr => "hppr:",
            HpprScheme::HpprSetup => "hppr-setup:",
            HpprScheme::HpprSandbox => "hppr-sandbox:",
            HpprScheme::HpprBrowse => "hppr-browse:",
            HpprScheme::HpprEditor => "hppr-editor:",
        }
    }

    /// Returns whether this scheme requires an endpoint ({via:...}).
    pub fn requires_endpoint(&self) -> bool {
        matches!(self, HpprScheme::HpprSetup | HpprScheme::HpprSandbox)
    }

    /// Returns whether this scheme forbids an endpoint.
    pub fn forbids_endpoint(&self) -> bool {
        matches!(self, HpprScheme::HpprEditor)
    }
}

/// Network endpoint with host and port.
///
/// Default port is 4777 (HPPR standard port).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    host: String,
    port: u16,
}

impl Endpoint {
    /// Default HPPR port.
    pub const DEFAULT_PORT: u16 = 4777;

    /// Create endpoint with explicit port.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    /// Parse from host string, defaulting to the given port if not specified.
    pub fn from_str_with_default(s: &str, default_port: u16) -> Self {
        // Handle IPv6 bracket form: [::1]:port or [::1]
        if s.starts_with('[') {
            if let Some(close) = s.find(']') {
                let host = &s[1..close];
                let rest = &s[close + 1..];
                if let Some(port_str) = rest.strip_prefix(':') {
                    if let Ok(port) = port_str.parse::<u16>() {
                        return Self { host: host.to_string(), port };
                    }
                }
                return Self { host: host.to_string(), port: default_port };
            }
        }
        // Find the last colon that's part of a port (not IPv6)
        if let Some(colon_pos) = s.rfind(':') {
            let after = &s[colon_pos + 1..];
            if let Ok(port) = after.parse::<u16>() {
                return Self {
                    host: s[..colon_pos].to_string(),
                    port,
                };
            }
        }
        Self {
            host: s.to_string(),
            port: default_port,
        }
    }

    /// Parse from host string, defaulting to port 4777 if not specified.
    pub fn from_str(s: &str) -> Self {
        Self::from_str_with_default(s, Self::DEFAULT_PORT)
    }

    /// Get the host.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Get the port.
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

/// Endpoint specification in HAVIAddress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointKind {
    /// No endpoint specified
    None,
    /// Explicit host:port endpoint (from {via:host:port} in JSONqa)
    Direct(Endpoint),
}

impl EndpointKind {
    /// Returns true if this is an explicit endpoint.
    pub fn is_specified(&self) -> bool {
        !matches!(self, EndpointKind::None)
    }

    /// Returns the direct endpoint if present.
    pub fn as_direct(&self) -> Option<&Endpoint> {
        match self {
            EndpointKind::Direct(ep) => Some(ep),
            _ => None,
        }
    }
}

impl fmt::Display for EndpointKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EndpointKind::None => Ok(()),
            EndpointKind::Direct(ep) => write!(f, "{}", ep),
        }
    }
}

/// HPPR URL parse error.
#[derive(Debug, Clone)]
pub enum HpprUrlParseError {
    /// Unknown or unsupported scheme.
    UnknownScheme(String),
    /// Missing // coordinate marker.
    MissingCoordinate,
    /// Endpoint is required for this scheme.
    MissingEndpoint,
    /// Endpoint is not allowed for this scheme.
    EndpointNotAllowed,
    /// Invalid URC format.
    InvalidUrc(String),
    /// Invalid havi:// URL format.
    InvalidHaviUrl(String),
}

impl fmt::Display for HpprUrlParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownScheme(s) => write!(f, "Unknown scheme: {}", s),
            Self::MissingCoordinate => write!(f, "Missing // coordinate marker"),
            Self::MissingEndpoint => write!(f, "Endpoint required for this scheme"),
            Self::EndpointNotAllowed => write!(f, "Endpoint not allowed for this scheme"),
            Self::InvalidUrc(e) => write!(f, "Invalid URC: {}", e),
            Self::InvalidHaviUrl(e) => write!(f, "Invalid havi:// URL: {}", e),
        }
    }
}

impl std::error::Error for HpprUrlParseError {}

/// Build a URL with `{via:...}` suffix.
///
/// `coord` is a full coordinate like `hppr://group/app/loc`.
/// `via` is the endpoint like `192.168.1.5:4777`.
pub fn via_url(coord: &str, via: &str) -> String {
    format!("{}{{via:{}}}", coord, via)
}

/// Extract `via` value from JSONqa string, returning (via_value, remaining_qa).
///
/// The JSONqa string starts with `{` and ends with `}`.
/// Keys are `key:value` pairs separated by `,`.
/// Returns (Some(via_value), remaining_qa_or_empty) if `via:` key found.
fn extract_via(jsonqa: &str) -> (Option<String>, String) {
    // Strip outer braces
    let inner = if jsonqa.starts_with('{') && jsonqa.ends_with('}') {
        &jsonqa[1..jsonqa.len() - 1]
    } else {
        return (None, jsonqa.to_string());
    };

    if inner.is_empty() {
        return (None, String::new());
    }

    // Split by comma and find the via entry
    let mut via_value = None;
    let mut remaining = Vec::new();

    for part in inner.split(',') {
        if let Some(val) = part.strip_prefix("via:") {
            via_value = Some(val.to_string());
        } else {
            remaining.push(part);
        }
    }

    let remaining_str = if remaining.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", remaining.join(","))
    };

    (via_value, remaining_str)
}

/// Parsed HAVIAddress combining scheme, optional endpoint, and URC.
///
/// Format: `scheme://group/app/location{via:host:port}`
/// Endpoint is extracted from `{via:...}` JSONqa key, not from a prefix.
#[derive(Debug, Clone)]
pub struct HAVIAddress {
    scheme: HpprScheme,
    endpoint: EndpointKind,
    urc: URC,
}

impl HAVIAddress {
    /// Parse a HAVIAddress string.
    ///
    /// All schemes use `scheme://coord` format. Endpoint is extracted from
    /// `{via:host:port}` JSONqa suffix.
    pub fn parse(url: &str) -> Result<Self, HpprUrlParseError> {
        // Detect scheme (order matters - longer prefixes first)
        let (scheme, rest) = if let Some(r) = url.strip_prefix("hppr-editor:") {
            (HpprScheme::HpprEditor, r)
        } else if let Some(r) = url.strip_prefix("hppr-setup:") {
            (HpprScheme::HpprSetup, r)
        } else if let Some(r) = url.strip_prefix("hppr-sandbox:") {
            (HpprScheme::HpprSandbox, r)
        } else if let Some(r) = url.strip_prefix("hppr-browse:") {
            (HpprScheme::HpprBrowse, r)
        } else if let Some(r) = url.strip_prefix("hppr:") {
            (HpprScheme::Hppr, r)
        } else {
            let scheme = url.split(':').next().unwrap_or("").to_string();
            return Err(HpprUrlParseError::UnknownScheme(scheme));
        };

        // All schemes: rest must start with //
        if !rest.starts_with("//") {
            return Err(HpprUrlParseError::MissingCoordinate);
        }

        // Find JSONqa suffix ({...}) - check both literal and percent-encoded
        let (coord_str, via_value) = {
            let brace_pos = rest
                .find('{')
                .or_else(|| rest.find("%7B"));
            if let Some(idx) = brace_pos {
                let jsonqa_raw = &rest[idx..];
                // Percent-decode the JSONqa portion
                let jsonqa = if jsonqa_raw.contains('%') {
                    percent_decode_braces(jsonqa_raw)
                } else {
                    jsonqa_raw.to_string()
                };
                let (via, _remaining) = extract_via(&jsonqa);
                (&rest[..idx], via)
            } else {
                (rest, None)
            }
        };

        // Validate endpoint requirements per scheme
        let endpoint = match via_value {
            Some(ref v) => {
                if scheme.forbids_endpoint() {
                    return Err(HpprUrlParseError::EndpointNotAllowed);
                }
                EndpointKind::Direct(Endpoint::from_str(v))
            },
            None => {
                if scheme.requires_endpoint() {
                    return Err(HpprUrlParseError::MissingEndpoint);
                }
                EndpointKind::None
            },
        };

        let urc = parse_urc(coord_str)?;

        Ok(Self {
            scheme,
            endpoint,
            urc,
        })
    }

    /// Get the scheme.
    pub fn scheme(&self) -> HpprScheme {
        self.scheme
    }

    /// Get the endpoint kind.
    pub fn endpoint_kind(&self) -> &EndpointKind {
        &self.endpoint
    }

    /// Get the direct endpoint if present.
    pub fn endpoint(&self) -> Option<&Endpoint> {
        self.endpoint.as_direct()
    }

    /// Get the endpoint as a display string (host:port or empty).
    pub fn endpoint_string(&self) -> Option<String> {
        match &self.endpoint {
            EndpointKind::None => None,
            EndpointKind::Direct(ep) => Some(ep.to_string()),
        }
    }

    /// Get the URC.
    pub fn urc(&self) -> &URC {
        &self.urc
    }

    /// Get the URC as a string.
    pub fn urc_string(&self) -> String {
        self.urc.to_string()
    }

    /// Returns true if this is a LIST request (trailing slash).
    pub fn is_listing(&self) -> bool {
        self.urc.is_listing()
    }

    /// Extract coordinate parts (group, app, location) with empty string defaults.
    ///
    /// Simplifies the common pattern of extracting fields without `unwrap_or_default()`.
    /// Location does NOT include trailing slash.
    pub fn parts(&self) -> CoordinateParts {
        self.urc.parts()
    }

    /// Get group from URC.
    pub fn group(&self) -> Option<String> {
        self.urc.group_app_loc().map(|(g, _)| g)
    }

    /// Get app from URC.
    pub fn app(&self) -> Option<String> {
        self.urc
            .group_app_loc()
            .and_then(|(_, rest)| rest.map(|(a, _)| a))
    }

    /// Get location from URC (without trailing slash).
    pub fn location(&self) -> Option<String> {
        self.urc
            .group_app_loc()
            .and_then(|(_, rest)| rest.and_then(|(_, loc)| loc))
    }

    /// Get location preserving trailing slash for LIST mode.
    ///
    /// Returns "/" if URL ends with / but has no explicit location (e.g., //group/app/).
    /// Returns "loc/" if URL ends with / (e.g., //group/app/loc/).
    /// Returns location as-is if no trailing slash.
    pub fn location_with_slash(&self) -> String {
        let loc = self.location().unwrap_or_default();
        if self.is_listing() && !loc.ends_with('/') {
            if loc.is_empty() {
                "/".to_string()
            } else {
                format!("{}/", loc)
            }
        } else {
            loc
        }
    }

    /// Check if this URL has a direct endpoint ({via:...} present).
    pub fn has_direct_endpoint(&self) -> bool {
        matches!(self.scheme, HpprScheme::Hppr | HpprScheme::HpprBrowse) &&
            matches!(self.endpoint, EndpointKind::Direct(_))
    }

    /// Build a URC string from group, app, location.
    ///
    /// Handles trailing slash for LIST mode.
    pub fn build_urc_string(group: &str, app: &str, location: &str) -> String {
        match (group.is_empty(), app.is_empty(), location.is_empty()) {
            (true, _, _) => "//".to_string(),
            (false, true, _) => format!("//{}/", group),
            (false, false, true) => format!("//{}/{}", group, app),
            (false, false, false) if location == "/" => format!("//{}/{}/", group, app),
            (false, false, false) => format!("//{}/{}/{}", group, app, location),
        }
    }

    /// Reconstruct the full URL from parts.
    ///
    /// Does NOT include `{via:...}` suffix. Endpoint info is metadata only,
    /// not preserved in URL joins or reconstructions.
    pub fn reconstruct(&self, new_coord: &str) -> String {
        format!("{}{}", self.scheme.prefix(), new_coord)
    }

    /// Returns true if this uses routed mode (no explicit endpoint).
    pub fn is_routed(&self) -> bool {
        matches!(self.scheme, HpprScheme::Hppr) &&
            matches!(self.endpoint, EndpointKind::None)
    }

    /// Returns true if this uses direct connection mode ({via:...} present).
    pub fn is_direct(&self) -> bool {
        self.has_direct_endpoint()
    }
}

/// Percent-decode only `%7B` and `%7D` (braces) in a string.
/// Decode percent-encoded JSONqa in an HPPR URL string for display.
///
/// Finds the JSONqa suffix (starting at `%7B` or `%7b`) and decodes
/// `%7B` → `{`, `%7D` → `}`, `%23` → `#` within it.
pub fn percent_decode_jsonqa(url: &str) -> String {
    let pos = url.find("%7B").or_else(|| url.find("%7b"));
    match pos {
        Some(idx) => {
            let (prefix, suffix) = url.split_at(idx);
            let decoded = suffix
                .replace("%7B", "{")
                .replace("%7D", "}")
                .replace("%7b", "{")
                .replace("%7d", "}")
                .replace("%23", "#");
            format!("{}{}", prefix, decoded)
        },
        None => url.to_owned(),
    }
}

fn percent_decode_braces(input: &str) -> String {
    input
        .replace("%7B", "{")
        .replace("%7D", "}")
        .replace("%7b", "{")
        .replace("%7d", "}")
}

/// Parse URC string, handling partial URCs gracefully.
///
/// The hppr_packet::URC::parse is strict about validation.
/// For browser URL bar input we may receive partial URCs like "//u/" which are valid
/// for LIST operations but may fail strict validation.
fn parse_urc(urc_str: &str) -> Result<URC, HpprUrlParseError> {
    // Ensure it starts with //
    if !urc_str.starts_with("//") {
        return Err(HpprUrlParseError::MissingCoordinate);
    }

    // Try parsing with hppr_packet::URC for proper validation
    URC::parse(urc_str.to_string()).map_err(|e| HpprUrlParseError::InvalidUrc(e.to_string()))
}

/// Parsed havi:// admin page URL.
///
/// Format: `havi:///path` (simple path-based, no URC)
/// Examples: `havi:///home-repo`, `havi:///homepage`, `havi:///ring0`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HaviUrl {
    path: String,
}

impl HaviUrl {
    /// Parse a havi:// URL.
    ///
    /// Expects format: `havi:///path` or `havi://path`
    pub fn parse(url: &str) -> Result<Self, HpprUrlParseError> {
        let rest = url.strip_prefix("havi:").ok_or_else(|| {
            HpprUrlParseError::UnknownScheme(
                url.split(':').next().unwrap_or("").to_string(),
            )
        })?;

        // Strip leading slashes to get the path
        let path = rest.trim_start_matches('/').to_string();
        Ok(Self { path })
    }

    /// Get the path (e.g., "home-repo", "home", "ring0").
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl fmt::Display for HaviUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "havi:///{}", self.path)
    }
}

/// Any HPPR-family URL.
///
/// Unified entry point for parsing all HPPR URL types.
#[derive(Debug, Clone)]
pub enum HpprUrl {
    /// URC-based URL (hppr://, hppr-setup:, hppr-sandbox:, hppr-browse://, hppr-editor://)
    HAVIAddress(HAVIAddress),
    /// Admin page URL (havi://)
    Havi(HaviUrl),
}

impl HpprUrl {
    /// Parse any HPPR-family URL.
    ///
    /// Detects the scheme and delegates to the appropriate parser.
    pub fn parse(url: &str) -> Result<Self, HpprUrlParseError> {
        if url.starts_with("havi:") {
            Ok(HpprUrl::Havi(HaviUrl::parse(url)?))
        } else {
            Ok(HpprUrl::HAVIAddress(HAVIAddress::parse(url)?))
        }
    }

    /// Check if a scheme string is an HPPR-family scheme.
    pub fn is_hppr_scheme(scheme: &str) -> bool {
        matches!(
            scheme,
            "hppr"
                | "hppr-setup"
                | "hppr-sandbox"
                | "hppr-browse"
                | "hppr-editor"
                | "havi"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_parsing() {
        let e = Endpoint::from_str("192.168.1.10");
        assert_eq!(e.host(), "192.168.1.10");
        assert_eq!(e.port(), 4777);

        let e = Endpoint::from_str("192.168.1.10:4778");
        assert_eq!(e.host(), "192.168.1.10");
        assert_eq!(e.port(), 4778);

        let e = Endpoint::from_str("localhost");
        assert_eq!(e.host(), "localhost");
        assert_eq!(e.port(), 4777);

        let e = Endpoint::from_str("localhost:8080");
        assert_eq!(e.host(), "localhost");
        assert_eq!(e.port(), 8080);
    }

    #[test]
    fn test_endpoint_display() {
        let e = Endpoint::new("localhost", 4777);
        assert_eq!(e.to_string(), "localhost:4777");
    }

    #[test]
    fn test_hppr_routed() {
        let url = HAVIAddress::parse("hppr://chess/game/board.html").unwrap();
        assert_eq!(url.scheme(), HpprScheme::Hppr);
        assert!(url.endpoint().is_none());
        assert_eq!(url.group(), Some("chess".to_string()));
        assert_eq!(url.app(), Some("game".to_string()));
        assert_eq!(url.location(), Some("board.html".to_string()));
        assert!(!url.is_listing());
    }

    #[test]
    fn test_hppr_with_jsonqa_suffix() {
        // JSONqa suffix must be stripped before URC parsing
        let url = HAVIAddress::parse("hppr://u/showcase/video/index.html{src://u/showcase/video/demo.mp4}").unwrap();
        assert_eq!(url.scheme(), HpprScheme::Hppr);
        assert_eq!(url.group(), Some("u".to_string()));
        assert_eq!(url.app(), Some("showcase".to_string()));
        assert_eq!(url.location(), Some("video/index.html".to_string()));
    }

    #[test]
    fn test_hppr_with_percent_encoded_jsonqa() {
        // The url crate percent-encodes { -> %7B, } -> %7D
        let url = HAVIAddress::parse("hppr://u/showcase/video/index.html%7Bsrc://u/showcase/video/demo.mp4%7D").unwrap();
        assert_eq!(url.group(), Some("u".to_string()));
        assert_eq!(url.app(), Some("showcase".to_string()));
        assert_eq!(url.location(), Some("video/index.html".to_string()));
    }

    #[test]
    fn test_hppr_with_via() {
        let url = HAVIAddress::parse("hppr://chess/game/board.html{via:192.168.1.5:4777}").unwrap();
        assert_eq!(url.scheme(), HpprScheme::Hppr);
        assert!(url.has_direct_endpoint());
        let ep = url.endpoint().unwrap();
        assert_eq!(ep.host(), "192.168.1.5");
        assert_eq!(ep.port(), 4777);
        assert_eq!(url.group(), Some("chess".to_string()));
    }

    #[test]
    fn test_hppr_via_default_port() {
        let url = HAVIAddress::parse("hppr://chess/game/{via:192.168.1.5}").unwrap();
        let ep = url.endpoint().unwrap();
        assert_eq!(ep.host(), "192.168.1.5");
        assert_eq!(ep.port(), 4777); // Default port
        assert!(url.is_listing());
    }

    #[test]
    fn test_hppr_editor() {
        let url = HAVIAddress::parse("hppr-editor://u/web/index.html").unwrap();
        assert_eq!(url.scheme(), HpprScheme::HpprEditor);
        assert!(url.endpoint().is_none());
        assert_eq!(url.group(), Some("u".to_string()));
        assert_eq!(url.app(), Some("web".to_string()));
        assert_eq!(url.location(), Some("index.html".to_string()));
    }

    #[test]
    fn test_hppr_editor_rejects_endpoint() {
        let result = HAVIAddress::parse("hppr-editor://u/web/index.html{via:127.0.0.1}");
        assert!(matches!(result, Err(HpprUrlParseError::EndpointNotAllowed)));
    }

    #[test]
    fn test_hppr_setup() {
        let url = HAVIAddress::parse("hppr-setup://chess/game/{via:192.168.1.10}").unwrap();
        assert_eq!(url.scheme(), HpprScheme::HpprSetup);
        let ep = url.endpoint().unwrap();
        assert_eq!(ep.host(), "192.168.1.10");
        assert_eq!(ep.port(), 4777);
        assert_eq!(url.group(), Some("chess".to_string()));
        assert_eq!(url.app(), Some("game".to_string()));
    }

    #[test]
    fn test_hppr_setup_requires_endpoint() {
        let result = HAVIAddress::parse("hppr-setup://chess/game/");
        assert!(matches!(result, Err(HpprUrlParseError::MissingEndpoint)));
    }

    #[test]
    fn test_hppr_sandbox() {
        let url = HAVIAddress::parse("hppr-sandbox://group/app/index.html{via:10.0.0.5:4778}").unwrap();
        assert_eq!(url.scheme(), HpprScheme::HpprSandbox);
        let ep = url.endpoint().unwrap();
        assert_eq!(ep.host(), "10.0.0.5");
        assert_eq!(ep.port(), 4778);
    }

    #[test]
    fn test_hppr_browse() {
        let url = HAVIAddress::parse("hppr-browse://chess/game/assets/").unwrap();
        assert_eq!(url.scheme(), HpprScheme::HpprBrowse);
        assert!(url.endpoint().is_none());
        assert!(url.is_listing());
    }

    #[test]
    fn test_hppr_browse_with_via() {
        let url = HAVIAddress::parse("hppr-browse://chess/game/assets/{via:192.168.1.5}").unwrap();
        assert_eq!(url.scheme(), HpprScheme::HpprBrowse);
        let ep = url.endpoint().unwrap();
        assert_eq!(ep.host(), "192.168.1.5");
        assert_eq!(ep.port(), 4777);
        assert!(url.is_listing());
    }

    #[test]
    fn test_unknown_scheme() {
        let result = HAVIAddress::parse("http://example.com");
        assert!(matches!(result, Err(HpprUrlParseError::UnknownScheme(_))));
    }

    #[test]
    fn test_havi_not_supported_by_host_urc() {
        let result = HAVIAddress::parse("havi:///home-repo");
        assert!(matches!(result, Err(HpprUrlParseError::UnknownScheme(_))));
    }

    #[test]
    fn test_build_urc_string() {
        assert_eq!(HAVIAddress::build_urc_string("", "", ""), "//");
        assert_eq!(HAVIAddress::build_urc_string("u", "", ""), "//u/");
        assert_eq!(HAVIAddress::build_urc_string("u", "app", ""), "//u/app");
        assert_eq!(HAVIAddress::build_urc_string("u", "app", "loc"), "//u/app/loc");
        assert_eq!(HAVIAddress::build_urc_string("u", "app", "/"), "//u/app/");
        assert_eq!(
            HAVIAddress::build_urc_string("u", "app", "loc/"),
            "//u/app/loc/"
        );
    }

    #[test]
    fn test_listing_detection() {
        let url = HAVIAddress::parse("hppr://chess/game/").unwrap();
        assert!(url.is_listing());

        let url = HAVIAddress::parse("hppr://chess/game/board.html").unwrap();
        assert!(!url.is_listing());
    }

    #[test]
    fn test_havi_url_parse() {
        let url = HaviUrl::parse("havi:///home-repo").unwrap();
        assert_eq!(url.path(), "home-repo");

        let url = HaviUrl::parse("havi:///homepage").unwrap();
        assert_eq!(url.path(), "homepage");

        let url = HaviUrl::parse("havi:///ring0").unwrap();
        assert_eq!(url.path(), "ring0");
    }

    #[test]
    fn test_havi_url_display() {
        let url = HaviUrl::parse("havi:///home-repo").unwrap();
        assert_eq!(url.to_string(), "havi:///home-repo");
    }

    #[test]
    fn test_hppr_url_parse_havi() {
        let url = HpprUrl::parse("havi:///home-repo").unwrap();
        match url {
            HpprUrl::Havi(havi) => assert_eq!(havi.path(), "home-repo"),
            _ => panic!("Expected HaviUrl"),
        }
    }

    #[test]
    fn test_hppr_url_parse_havi_address() {
        let url = HpprUrl::parse("hppr://chess/game/board.html").unwrap();
        match url {
            HpprUrl::HAVIAddress(address) => {
                assert_eq!(address.scheme(), HpprScheme::Hppr);
                assert_eq!(address.group(), Some("chess".to_string()));
            },
            _ => panic!("Expected HAVIAddress"),
        }
    }

    #[test]
    fn test_is_hppr_scheme() {
        assert!(HpprUrl::is_hppr_scheme("hppr"));
        assert!(HpprUrl::is_hppr_scheme("hppr-setup"));
        assert!(HpprUrl::is_hppr_scheme("hppr-sandbox"));
        assert!(HpprUrl::is_hppr_scheme("hppr-browse"));
        assert!(HpprUrl::is_hppr_scheme("hppr-editor"));
        assert!(HpprUrl::is_hppr_scheme("havi"));
        assert!(!HpprUrl::is_hppr_scheme("http"));
        assert!(!HpprUrl::is_hppr_scheme("https"));
        assert!(!HpprUrl::is_hppr_scheme("hppr-local"));
        assert!(!HpprUrl::is_hppr_scheme("hpprs"));
    }

    #[test]
    fn test_reconstruct() {
        // Reconstruct does NOT include {via:...}
        let url = HAVIAddress::parse("hppr://chess/game/board.html{via:192.168.1.5}").unwrap();
        assert_eq!(
            url.reconstruct("//other/app/index.html"),
            "hppr://other/app/index.html"
        );

        let url = HAVIAddress::parse("hppr://chess/game/board.html").unwrap();
        assert_eq!(
            url.reconstruct("//other/app/index.html"),
            "hppr://other/app/index.html"
        );
    }

    #[test]
    fn test_parts() {
        let url = HAVIAddress::parse("hppr://chess/game/board.html").unwrap();
        let parts = url.parts();
        assert_eq!(parts.group, "chess");
        assert_eq!(parts.app, "game");
        assert_eq!(parts.location, "board.html");

        let url = HAVIAddress::parse("hppr-setup://chess/game/{via:192.168.1.10}").unwrap();
        let parts = url.parts();
        assert_eq!(parts.group, "chess");
        assert_eq!(parts.app, "game");
        assert_eq!(parts.location, "");
    }

    #[test]
    fn test_hppr_routed_mode() {
        let url = HAVIAddress::parse("hppr://chess/game/board.html").unwrap();
        assert_eq!(url.scheme(), HpprScheme::Hppr);
        assert!(url.is_routed());
        assert!(!url.is_direct());
        assert_eq!(url.group(), Some("chess".to_string()));
    }

    #[test]
    fn test_hppr_direct_mode() {
        let url = HAVIAddress::parse("hppr://chess/game/board.html{via:192.168.1.5}").unwrap();
        assert!(url.is_direct());
        assert!(!url.is_routed());
    }

    #[test]
    fn test_endpoint_kind_display() {
        assert_eq!(EndpointKind::None.to_string(), "");
        assert_eq!(
            EndpointKind::Direct(Endpoint::new("localhost", 4777)).to_string(),
            "localhost:4777"
        );
    }

    #[test]
    fn test_via_url() {
        assert_eq!(
            via_url("hppr://chess/game/board.html", "192.168.1.5:4777"),
            "hppr://chess/game/board.html{via:192.168.1.5:4777}"
        );
    }

    #[test]
    fn test_via_with_other_qa() {
        // {via:...} alongside other JSONqa keys
        let url = HAVIAddress::parse("hppr://u/app/index.html{via:10.0.0.1:4777,src://u/app/data}").unwrap();
        assert_eq!(url.scheme(), HpprScheme::Hppr);
        let ep = url.endpoint().unwrap();
        assert_eq!(ep.host(), "10.0.0.1");
        assert_eq!(ep.port(), 4777);
        assert_eq!(url.group(), Some("u".to_string()));
    }

    #[test]
    fn test_extract_via() {
        let (via, rest) = extract_via("{via:192.168.1.5:4777}");
        assert_eq!(via, Some("192.168.1.5:4777".to_string()));
        assert_eq!(rest, "");

        let (via, rest) = extract_via("{via:host,other:val}");
        assert_eq!(via, Some("host".to_string()));
        assert_eq!(rest, "{other:val}");

        let (via, rest) = extract_via("{other:val}");
        assert!(via.is_none());
        assert_eq!(rest, "{other:val}");

        let (via, rest) = extract_via("{other:val,via:h:4777,more:x}");
        assert_eq!(via, Some("h:4777".to_string()));
        assert_eq!(rest, "{other:val,more:x}");
    }

    #[test]
    fn test_hppr_browse_with_via_has_direct_endpoint() {
        let url = HAVIAddress::parse("hppr-browse://chess/game/assets/{via:192.168.1.5}").unwrap();
        assert!(url.has_direct_endpoint());
    }

    #[test]
    fn test_hppr_browse_listing_with_via() {
        let url = HAVIAddress::parse("hppr-browse://u/web/{via:10.0.0.1}").unwrap();
        assert_eq!(url.scheme(), HpprScheme::HpprBrowse);
        let ep = url.endpoint().unwrap();
        assert_eq!(ep.host(), "10.0.0.1");
        assert!(url.is_listing());
    }
}
