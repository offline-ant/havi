/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR URL types for HAVI protocol handlers.
//!
//! Provides unified parsing for all HPPR-family URLs:
//! - `hppr://`, `hppr-sandbox:`, `hppr-browse://` - URC-based URLs
//! - `havi://` - Admin page URLs with simple path format
//!
//! Endpoint is specified via `{via:host:port}` JSONqa suffix, not as a prefix.
//! Example: `hppr://chess/game//board.html{via:192.168.1.5:4777}`

use hppr_packet::CoordinateParts;
use hppr_packet::urc::URC;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HpprScheme {
    Hppr,
    HpprSandbox,
    HpprBrowse,
}

impl HpprScheme {
    pub fn prefix(&self) -> &'static str {
        match self {
            HpprScheme::Hppr => "hppr:",
            HpprScheme::HpprSandbox => "hppr-sandbox:",
            HpprScheme::HpprBrowse => "hppr-browse:",
        }
    }

    pub fn requires_endpoint(&self) -> bool {
        matches!(self, HpprScheme::HpprSandbox)
    }

    pub fn forbids_endpoint(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    host: String,
    port: u16,
}

impl Endpoint {
    pub const DEFAULT_PORT: u16 = 4777;

    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    pub fn from_str_with_default(s: &str, default_port: u16) -> Self {
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

    pub fn from_str(s: &str) -> Self {
        Self::from_str_with_default(s, Self::DEFAULT_PORT)
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointKind {
    None,
    Direct(Endpoint),
}

impl EndpointKind {
    pub fn is_specified(&self) -> bool {
        !matches!(self, EndpointKind::None)
    }

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

#[derive(Debug, Clone)]
pub enum HpprUrlParseError {
    UnknownScheme(String),
    MissingCoordinate,
    MissingEndpoint,
    EndpointNotAllowed,
    InvalidUrc(String),
}

impl fmt::Display for HpprUrlParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownScheme(s) => write!(f, "Unknown scheme: {}", s),
            Self::MissingCoordinate => write!(f, "Missing // coordinate marker"),
            Self::MissingEndpoint => write!(f, "Endpoint required for this scheme"),
            Self::EndpointNotAllowed => write!(f, "Endpoint not allowed for this scheme"),
            Self::InvalidUrc(e) => write!(f, "Invalid URC: {}", e),
        }
    }
}

impl std::error::Error for HpprUrlParseError {}

pub fn via_url(coord: &str, via: &str) -> String {
    format!("{}{{via:{}}}", coord, via)
}

fn extract_via(jsonqa: &str) -> (Option<String>, String) {
    let inner = if jsonqa.starts_with('{') && jsonqa.ends_with('}') {
        &jsonqa[1..jsonqa.len() - 1]
    } else {
        return (None, jsonqa.to_string());
    };

    if inner.is_empty() {
        return (None, String::new());
    }

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

#[derive(Debug, Clone)]
pub struct HAVIAddress {
    scheme: HpprScheme,
    endpoint: EndpointKind,
    urc: URC,
}

impl HAVIAddress {
    pub fn parse(url: &str) -> Result<Self, HpprUrlParseError> {
        let (scheme, rest) = if let Some(r) = url.strip_prefix("hppr-sandbox:") {
            (HpprScheme::HpprSandbox, r)
        } else if let Some(r) = url.strip_prefix("hppr-browse:") {
            (HpprScheme::HpprBrowse, r)
        } else if let Some(r) = url.strip_prefix("hppr:") {
            (HpprScheme::Hppr, r)
        } else {
            let scheme = url.split(':').next().unwrap_or("").to_string();
            return Err(HpprUrlParseError::UnknownScheme(scheme));
        };

        if !rest.starts_with("//") {
            return Err(HpprUrlParseError::MissingCoordinate);
        }

        let (coord_str, via_value) = {
            let brace_pos = rest
                .find('{')
                .or_else(|| rest.find("%7B"));
            if let Some(idx) = brace_pos {
                let jsonqa_raw = &rest[idx..];
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

    pub fn scheme(&self) -> HpprScheme {
        self.scheme
    }

    pub fn endpoint_kind(&self) -> &EndpointKind {
        &self.endpoint
    }

    pub fn endpoint(&self) -> Option<&Endpoint> {
        self.endpoint.as_direct()
    }

    pub fn endpoint_string(&self) -> Option<String> {
        match &self.endpoint {
            EndpointKind::None => None,
            EndpointKind::Direct(ep) => Some(ep.to_string()),
        }
    }

    pub fn urc(&self) -> &URC {
        &self.urc
    }

    pub fn urc_string(&self) -> String {
        self.urc.to_string()
    }

    pub fn is_listing(&self) -> bool {
        self.urc.is_listing()
    }

    pub fn parts(&self) -> CoordinateParts {
        self.urc.parts()
    }

    pub fn group(&self) -> Option<String> {
        self.urc.group_api_key().map(|(g, _)| g)
    }

    pub fn api(&self) -> Option<String> {
        self.urc
            .group_api_key()
            .and_then(|(_, rest)| rest.map(|(api, _)| api))
    }

    pub fn key(&self) -> Option<String> {
        self.urc
            .group_api_key()
            .and_then(|(_, rest)| rest.and_then(|(_, key)| key))
    }

    pub fn key_with_slash(&self) -> String {
        let key = self.key().unwrap_or_default();
        if self.is_listing() && !key.ends_with('/') {
            if key.is_empty() {
                "/".to_string()
            } else {
                format!("{}/", key)
            }
        } else {
            key
        }
    }

    pub fn has_direct_endpoint(&self) -> bool {
        matches!(self.scheme, HpprScheme::Hppr | HpprScheme::HpprBrowse) &&
            matches!(self.endpoint, EndpointKind::Direct(_))
    }

    pub fn build_urc_string(group: &str, api: &str, key: &str) -> String {
        match (group.is_empty(), api.is_empty(), key.is_empty()) {
            (true, _, _) => "//".to_string(),
            (false, true, _) => format!("//{}/", group),
            (false, false, true) => format!("//{}/{}", group, api),
            (false, false, false) if key == "/" => format!("//{}/{}//", group, api),
            (false, false, false) => format!("//{}/{}//{}", group, api, key),
        }
    }

    pub fn reconstruct(&self, new_coord: &str) -> String {
        format!("{}{}", self.scheme.prefix(), new_coord)
    }

    pub fn is_routed(&self) -> bool {
        matches!(self.scheme, HpprScheme::Hppr) &&
            matches!(self.endpoint, EndpointKind::None)
    }

    pub fn is_direct(&self) -> bool {
        self.has_direct_endpoint()
    }
}

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

fn parse_urc(urc_str: &str) -> Result<URC, HpprUrlParseError> {
    if !urc_str.starts_with("//") {
        return Err(HpprUrlParseError::MissingCoordinate);
    }

    URC::parse(urc_str.to_string()).map_err(|e| HpprUrlParseError::InvalidUrc(e.to_string()))
}
