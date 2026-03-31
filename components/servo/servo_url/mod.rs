/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![deny(unsafe_code)]

pub mod encoding;
pub mod hppr;
pub mod origin;

use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;
use std::ops::{Index, Range, RangeFrom, RangeFull, RangeTo};
use std::path::Path;
use std::str::FromStr;

use hppr_packet::urc::URC;
use malloc_size_of::{MallocSizeOf, MallocSizeOfOps};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use servo_arc::Arc;
pub use url::Host;
use url::{Position, Url};
use uuid::Uuid;

pub use self::origin::{ImmutableOrigin, MutableOrigin, OpaqueOrigin, OriginSnapshot};
pub use hppr::{Endpoint, HAVIAddress, HpprScheme, HpprUrlParseError, via_url};
pub use hppr_packet::CoordinateParts;

/// Compute the origin for a HAVIAddress.
///
/// HPPR origins are isolated by group#app, not by host. This means:
/// - `//alice/photos` and `//alice/blog` are cross-origin (different apps)
/// - Two different servers hosting `//alice/photos` are same-origin (same group+app)
///
/// This prevents a malicious server from accessing another app's data within
/// the same group. Changing this logic weakens the browser's security boundary.
fn hppr_origin_for_address(address: &HAVIAddress) -> Option<ImmutableOrigin> {
    let urc = address.urc();
    let mut target_parts = urc.target_parts();
    let group = target_parts.next()?;
    let app = target_parts.next()?;
    // works because '#' is invalid in group and app
    let identity = format!("{group}#{app}");

    let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, identity.as_bytes());
    let host = Host::Domain(format!("hppr-{}", uuid.simple()));

    Some(ImmutableOrigin::Tuple(
        address.scheme().prefix().trim_end_matches(':').to_string(),
        host,
        0,
    ))
}

const DATA_URL_DISPLAY_LENGTH: usize = 40;

#[derive(Debug)]
pub enum UrlError {
    SetUsername,
    SetIpHost,
    SetPassword,
    ToFilePath,
    FromFilePath,
}

// ── HpprUrlData ──────────────────────────────────────────────────────

/// Parsed HPPR URL data stored in the `BrowserUrl::Hppr` variant.
///
/// Stores the raw UTF-8 URL string alongside its parsed components.
/// No percent-encoding is applied — `{`, `}`, `#` survive as-is.
#[derive(Clone, Debug)]
pub struct HpprUrlData {
    /// Full URL string: `"hppr://group/app/location{jsonqa}"`
    raw: String,
    /// Parsed scheme + endpoint + URC.
    address: HAVIAddress,
    /// JSONqa suffix including braces, or empty.
    jsonqa: String,
}

impl HpprUrlData {
    /// Parse an HPPR URL string into data.
    fn parse(input: &str) -> Result<Self, url::ParseError> {
        // Pass full URL to HAVIAddress::parse — it handles {via:...} and JSONqa.
        let address = HAVIAddress::parse(input)
            .map_err(|_| url::ParseError::InvalidDomainCharacter)?;
        let (_, jsonqa) = split_jsonqa(input);
        Ok(Self {
            raw: input.to_owned(),
            address,
            jsonqa: jsonqa.to_owned(),
        })
    }

    /// The full raw URL string.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// The parsed HAVIAddress.
    pub fn address(&self) -> &HAVIAddress {
        &self.address
    }

    /// The JSONqa suffix (including braces), or empty.
    pub fn jsonqa(&self) -> &str {
        &self.jsonqa
    }
}

impl PartialEq for HpprUrlData {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}
impl Eq for HpprUrlData {}

impl Hash for HpprUrlData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl PartialOrd for HpprUrlData {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HpprUrlData {
    fn cmp(&self, other: &Self) -> Ordering {
        self.raw.cmp(other.raw)
    }
}

impl MallocSizeOf for HpprUrlData {
    fn size_of(&self, _ops: &mut MallocSizeOfOps) -> usize {
        self.raw.len() + self.jsonqa.len()
    }
}

// ── BrowserUrl ───────────────────────────────────────────────────────

/// URL type for the HAVI browser.
///
/// Replaces `BrowserUrl` (`Arc<Url>`). HPPR URLs are stored natively as
/// UTF-8 strings without percent-encoding damage. Web URLs wrap `url::Url`.
#[derive(Clone)]
pub enum BrowserUrl {
    /// Standard web URL (http, https, about, data, file, etc.).
    Web(#[allow(unused)] Arc<Url>),
    /// HPPR-family URL (hppr, hppr-sandbox, hppr-setup, hppr-browse, hppr-editor, hppr-join, havi).
    Hppr(Arc<HpprUrlData>),
}

impl BrowserUrl {
    pub fn from_url(url: Url) -> Self {
        // Detect HPPR URLs coming through the Url path and convert them.
        let scheme = url.scheme();
        if scheme == "hppr" || scheme.starts_with("hppr-") {
            // Percent-decode JSONqa characters that were encoded to survive Url::parse.
            let decoded = hppr::percent_decode_jsonqa(url.as_str());
            if let Ok(data) = HpprUrlData::parse(&decoded) {
                return BrowserUrl::Hppr(Arc::new(data));
            }
        }
        BrowserUrl::Web(Arc::new(url))
    }

    pub fn parse_with_base(base: Option<&Self>, input: &str) -> Result<Self, url::ParseError> {
        if is_hppr_input(input) {
            return HpprUrlData::parse(input).map(|d| BrowserUrl::Hppr(Arc::new(d)));
        }
        let base_url: Option<&Url> = base.and_then(|b| b.as_web_url());
        Url::options()
            .base_url(base_url)
            .parse(input)
            .map(Self::from_url)
    }

    pub fn into_string(self) -> String {
        match self {
            BrowserUrl::Web(u) => String::from((*u).clone()),
            BrowserUrl::Hppr(d) => d.raw.clone(),
        }
    }

    pub fn into_url(self) -> Url {
        match self {
            BrowserUrl::Web(u) => (*u).clone(),
            BrowserUrl::Hppr(d) => {
                // Construct a Url for HTTP-only code paths.
                // Percent-encodes JSONqa characters to survive Url::parse.
                let encoded = encode_for_url_parse(&d.raw);
                Url::parse(&encoded).expect("BrowserUrl::Hppr should round-trip through Url")
            },
        }
    }

    pub fn get_arc(&self) -> Arc<Url> {
        match self {
            BrowserUrl::Web(u) => u.clone(),
            BrowserUrl::Hppr(d) => {
                let encoded = encode_for_url_parse(&d.raw);
                Arc::new(Url::parse(&encoded).expect("BrowserUrl::Hppr should round-trip"))
            },
        }
    }

    pub fn as_url(&self) -> &Url {
        match self {
            BrowserUrl::Web(u) => u,
            BrowserUrl::Hppr(_) => panic!("as_url() called on HPPR URL — use BrowserUrl methods"),
        }
    }

    /// Returns a reference to the inner `Url` if this is a web URL, or `None` for HPPR.
    pub fn as_web_url(&self) -> Option<&Url> {
        match self {
            BrowserUrl::Web(u) => Some(u),
            BrowserUrl::Hppr(_) => None,
        }
    }

    /// Returns the `HpprUrlData` if this is an HPPR URL.
    pub fn as_hppr(&self) -> Option<&HpprUrlData> {
        match self {
            BrowserUrl::Hppr(d) => Some(d),
            BrowserUrl::Web(_) => None,
        }
    }

    pub fn parse(input: &str) -> Result<Self, url::ParseError> {
        if is_hppr_input(input) {
            return HpprUrlData::parse(input).map(|d| BrowserUrl::Hppr(Arc::new(d)));
        }
        Url::parse(input).map(Self::from_url)
    }

    pub fn cannot_be_a_base(&self) -> bool {
        match self {
            BrowserUrl::Web(u) => u.cannot_be_a_base(),
            BrowserUrl::Hppr(_) => false,
        }
    }

    pub fn domain(&self) -> Option<&str> {
        match self {
            BrowserUrl::Web(u) => u.domain(),
            BrowserUrl::Hppr(_) => None,
        }
    }

    pub fn fragment(&self) -> Option<&str> {
        match self {
            BrowserUrl::Web(u) => u.fragment(),
            BrowserUrl::Hppr(_) => None,
        }
    }

    pub fn path(&self) -> &str {
        match self {
            BrowserUrl::Web(u) => u.path(),
            BrowserUrl::Hppr(d) => {
                let s = d.address.urc().as_ref();
                if s.starts_with("//") { s } else { "" }
            },
        }
    }

    pub fn origin(&self) -> ImmutableOrigin {
        match self {
            BrowserUrl::Hppr(d) => {
                hppr_origin_for_address(&d.address)
                    .unwrap_or_else(|| ImmutableOrigin::new_opaque())
            },
            BrowserUrl::Web(u) => ImmutableOrigin::new(u.origin()),
        }
    }

    pub fn scheme(&self) -> &str {
        match self {
            BrowserUrl::Web(u) => u.scheme(),
            BrowserUrl::Hppr(d) => d.address.scheme().prefix().trim_end_matches(':'),
        }
    }

    pub fn is_secure_scheme(&self) -> bool {
        let scheme = self.scheme();
        scheme == "https" || scheme == "wss"
    }

    pub fn is_local_scheme(&self) -> bool {
        let scheme = self.scheme();
        scheme == "about" || scheme == "blob" || scheme == "data"
    }

    pub fn is_special_scheme(&self) -> bool {
        let scheme = self.scheme();
        scheme == "ftp" ||
            scheme == "file" ||
            scheme == "http" ||
            scheme == "https" ||
            scheme == "ws" ||
            scheme == "wss"
    }

    pub fn is_equal_excluding_fragments(&self, other: &BrowserUrl) -> bool {
        match (self, other) {
            (BrowserUrl::Web(a), BrowserUrl::Web(b)) => {
                a[..Position::AfterQuery] == b[..Position::AfterQuery]
            },
            (BrowserUrl::Hppr(a), BrowserUrl::Hppr(b)) => a.raw == b.raw,
            _ => false,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            BrowserUrl::Web(u) => u.as_str(),
            BrowserUrl::Hppr(d) => &d.raw,
        }
    }

    pub fn as_mut_url(&mut self) -> &mut Url {
        match self {
            BrowserUrl::Web(u) => Arc::make_mut(u),
            BrowserUrl::Hppr(_) => {
                panic!("as_mut_url() called on HPPR URL — only valid for web URLs")
            },
        }
    }

    pub fn set_username(&mut self, user: &str) -> Result<(), UrlError> {
        self.as_mut_url()
            .set_username(user)
            .map_err(|_| UrlError::SetUsername)
    }

    pub fn set_ip_host(&mut self, addr: IpAddr) -> Result<(), UrlError> {
        self.as_mut_url()
            .set_ip_host(addr)
            .map_err(|_| UrlError::SetIpHost)
    }

    pub fn set_password(&mut self, pass: Option<&str>) -> Result<(), UrlError> {
        self.as_mut_url()
            .set_password(pass)
            .map_err(|_| UrlError::SetPassword)
    }

    pub fn set_fragment(&mut self, fragment: Option<&str>) {
        self.as_mut_url().set_fragment(fragment)
    }

    pub fn username(&self) -> &str {
        match self {
            BrowserUrl::Web(u) => u.username(),
            BrowserUrl::Hppr(_) => "",
        }
    }

    pub fn password(&self) -> Option<&str> {
        match self {
            BrowserUrl::Web(u) => u.password(),
            BrowserUrl::Hppr(_) => None,
        }
    }

    pub fn to_file_path(&self) -> Result<::std::path::PathBuf, UrlError> {
        match self {
            BrowserUrl::Web(u) => u.to_file_path().map_err(|_| UrlError::ToFilePath),
            BrowserUrl::Hppr(_) => Err(UrlError::ToFilePath),
        }
    }

    pub fn host(&self) -> Option<url::Host<&str>> {
        match self {
            BrowserUrl::Web(u) => u.host(),
            BrowserUrl::Hppr(_) => None,
        }
    }

    pub fn host_str(&self) -> Option<&str> {
        match self {
            BrowserUrl::Web(u) => u.host_str(),
            BrowserUrl::Hppr(_) => None,
        }
    }

    pub fn port(&self) -> Option<u16> {
        match self {
            BrowserUrl::Web(u) => u.port(),
            BrowserUrl::Hppr(_) => None,
        }
    }

    pub fn port_or_known_default(&self) -> Option<u16> {
        match self {
            BrowserUrl::Web(u) => u.port_or_known_default(),
            BrowserUrl::Hppr(_) => None,
        }
    }

    pub fn join(&self, input: &str) -> Result<BrowserUrl, url::ParseError> {
        match self {
            BrowserUrl::Hppr(d) => self.join_hppr(input, &d.address),
            BrowserUrl::Web(_) => {
                if is_hppr_input(input) {
                    return BrowserUrl::parse(input);
                }
                match self {
                    BrowserUrl::Web(u) => u.join(input).map(Self::from_url),
                    _ => unreachable!(),
                }
            },
        }
    }

    fn join_hppr(
        &self,
        input: &str,
        address: &HAVIAddress,
    ) -> Result<BrowserUrl, url::ParseError> {
        let (coord_input, jsonqa) = split_jsonqa(input);

        if let Some(colon) = coord_input.find(':') {
            if colon > 0 &&
                !coord_input.starts_with('.') &&
                !coord_input.starts_with('/') &&
                coord_input[..colon]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                if is_hppr_input(coord_input) {
                    let full = if jsonqa.is_empty() {
                        coord_input.to_owned()
                    } else {
                        format!("{}{}", coord_input, jsonqa)
                    };
                    return BrowserUrl::parse(&full);
                }
                if let Ok(url) = Url::parse(coord_input) {
                    return Ok(Self::from_url(url));
                }
            }
        }

        if coord_input.is_empty() {
            let url_str = format!(
                "{}{}",
                address.reconstruct(&address.urc_string()),
                jsonqa,
            );
            return HpprUrlData::parse(&url_str)
                .map(|d| BrowserUrl::Hppr(Arc::new(d)));
        }

        if coord_input.starts_with("//") {
            let url_str = format!(
                "{}{}",
                address.reconstruct(coord_input),
                jsonqa,
            );
            return HpprUrlData::parse(&url_str)
                .map(|d| BrowserUrl::Hppr(Arc::new(d)));
        }

        let urc_string = address.urc_string();
        let base_coord = if urc_string.ends_with('/') {
            urc_string
        } else {
            match urc_string.rfind('/') {
                Some(pos) => format!("{}/", &urc_string[..pos]),
                None => urc_string,
            }
        };

        let Ok(current) = URC::parse(base_coord) else {
            return self.into_url_for_join().join(input).map(Self::from_url);
        };

        let Ok(resolved) = current.join(coord_input) else {
            return self.into_url_for_join().join(input).map(Self::from_url);
        };

        let url_str = format!(
            "{}{}",
            address.reconstruct(&resolved.unwrap()),
            jsonqa,
        );
        HpprUrlData::parse(&url_str).map(|d| BrowserUrl::Hppr(Arc::new(d)))
    }

    fn into_url_for_join(&self) -> Url {
        match self {
            BrowserUrl::Web(u) => u.as_ref().clone(),
            BrowserUrl::Hppr(d) => {
                let encoded = encode_for_url_parse(&d.raw);
                Url::parse(&encoded).unwrap()
            },
        }
    }

    pub fn path_segments(&self) -> Option<::std::str::Split<'_, char>> {
        match self {
            BrowserUrl::Web(u) => u.path_segments(),
            BrowserUrl::Hppr(_) => None,
        }
    }

    pub fn query(&self) -> Option<&str> {
        match self {
            BrowserUrl::Web(u) => u.query(),
            BrowserUrl::Hppr(_) => None,
        }
    }

    pub fn from_file_path<P: AsRef<Path>>(path: P) -> Result<Self, UrlError> {
        Url::from_file_path(path)
            .map(Self::from_url)
            .map_err(|_| UrlError::FromFilePath)
    }

    pub fn debug_compact(&self) -> impl std::fmt::Display + '_ {
        match self {
            BrowserUrl::Web(u) => {
                let s = match u.scheme() {
                    "http" | "https" => {
                        let mut st = u.as_str();
                        st = st.strip_prefix(u.scheme()).unwrap_or(st);
                        st = st.strip_prefix(':').unwrap_or(st);
                        st = st.trim_start_matches('/');
                        if st.is_empty() { u.as_str() } else { st }
                    },
                    "file" => {
                        let path = u.path();
                        let i = path.rfind('/');
                        let i = i.map(|i| path[..i].rfind('/').unwrap_or(i));
                        match i {
                            None | Some(0) => path,
                            Some(i) => &path[i + 1..],
                        }
                    },
                    _ => u.as_str(),
                };
                s
            },
            BrowserUrl::Hppr(d) => d.raw.as_str(),
        }
    }

    pub fn is_potentially_trustworthy(&self) -> bool {
        if self.as_str() == "about:blank" || self.as_str() == "about:srcdoc" {
            return true;
        }
        if self.scheme() == "data" {
            return true;
        }
        self.origin().is_potentially_trustworthy()
    }

    pub fn matches_about_blank(&self) -> bool {
        match self {
            BrowserUrl::Hppr(_) => false,
            BrowserUrl::Web(u) => {
                u.scheme() == "about" &&
                    u.path() == "blank" &&
                    u.username().is_empty() &&
                    u.password().is_none() &&
                    u.host().is_none()
            },
        }
    }

    pub fn url_without_fragment(&self) -> &str {
        match self {
            BrowserUrl::Web(u) => &u[..Position::AfterQuery],
            BrowserUrl::Hppr(d) => &d.raw,
        }
    }

    pub fn url_from_before_fragment(&self) -> &str {
        match self {
            BrowserUrl::Web(u) => &u[Position::BeforeFragment..],
            BrowserUrl::Hppr(d) => &d.raw,
        }
    }

    pub fn url_after_scheme(&self) -> &str {
        match self {
            BrowserUrl::Web(u) => &u[Position::AfterScheme..],
            BrowserUrl::Hppr(d) => {
                let prefix = d.address.scheme().prefix();
                &d.raw[prefix.len()..]
            },
        }
    }

    pub fn url_from_before_host(&self) -> &str {
        match self {
            BrowserUrl::Web(u) => &u[url::Position::BeforeHost..],
            BrowserUrl::Hppr(d) => {
                let prefix = d.address.scheme().prefix();
                &d.raw[prefix.len()..]
            },
        }
    }

    pub fn url_after_path(&self) -> &str {
        match self {
            BrowserUrl::Web(u) => &u[Position::AfterPath..],
            BrowserUrl::Hppr(d) => &d.jsonqa,
        }
    }

    pub fn equals_ignoring_fragment(&self, other: &BrowserUrl) -> bool {
        self.url_without_fragment() == other.url_without_fragment()
    }
}

impl fmt::Display for BrowserUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BrowserUrl::Web(u) => u.fmt(formatter),
            BrowserUrl::Hppr(d) => d.raw.fmt(formatter),
        }
    }
}

impl fmt::Debug for BrowserUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let url_string = self.as_str();
        if self.scheme() != "data" || url_string.len() <= DATA_URL_DISPLAY_LENGTH {
            return url_string.fmt(formatter);
        }

        let mut hasher = DefaultHasher::new();
        hasher.write(url_string.as_bytes());

        format!(
            "{}... ({:x})",
            url_string
                .chars()
                .take(DATA_URL_DISPLAY_LENGTH)
                .collect::<String>(),
            hasher.finish()
        )
        .fmt(formatter)
    }
}

impl PartialEq for BrowserUrl {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}
impl Eq for BrowserUrl {}

impl Hash for BrowserUrl {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl PartialOrd for BrowserUrl {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BrowserUrl {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Serialize for BrowserUrl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for BrowserUrl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BrowserUrlVisitor;
        impl<'de> Visitor<'de> for BrowserUrlVisitor {
            type Value = BrowserUrl;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a URL string")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<BrowserUrl, E> {
                BrowserUrl::parse(v).map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_str(BrowserUrlVisitor)
    }
}

impl MallocSizeOf for BrowserUrl {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        match self {
            BrowserUrl::Web(u) => u.size_of(ops),
            BrowserUrl::Hppr(d) => d.size_of(ops),
        }
    }
}

impl Index<RangeFull> for BrowserUrl {
    type Output = str;
    fn index(&self, _: RangeFull) -> &str {
        self.as_str()
    }
}

impl Index<RangeFrom<Position>> for BrowserUrl {
    type Output = str;
    fn index(&self, range: RangeFrom<Position>) -> &str {
        &self.as_url()[range]
    }
}

impl Index<RangeTo<Position>> for BrowserUrl {
    type Output = str;
    fn index(&self, range: RangeTo<Position>) -> &str {
        &self.as_url()[range]
    }
}

impl Index<Range<Position>> for BrowserUrl {
    type Output = str;
    fn index(&self, range: Range<Position>) -> &str {
        &self.as_url()[range]
    }
}

impl From<Url> for BrowserUrl {
    fn from(url: Url) -> Self {
        BrowserUrl::from_url(url)
    }
}

impl From<Arc<Url>> for BrowserUrl {
    fn from(url: Arc<Url>) -> Self {
        let scheme = url.scheme();
        if scheme == "hppr" || scheme.starts_with("hppr-") {
            let decoded = hppr::percent_decode_jsonqa(url.as_str());
            if let Ok(data) = HpprUrlData::parse(&decoded) {
                return BrowserUrl::Hppr(Arc::new(data));
            }
        }
        BrowserUrl::Web(url)
    }
}

impl FromStr for BrowserUrl {
    type Err = url::ParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        BrowserUrl::parse(value)
    }
}

fn is_hppr_input(input: &str) -> bool {
    input.starts_with("hppr:") ||
        input.starts_with("hppr-")
}

fn split_jsonqa(input: &str) -> (&str, &str) {
    let brace_pos = input
        .find('{')
        .or_else(|| input.find("%7B").or_else(|| input.find("%7b")));
    match brace_pos {
        Some(pos) => (&input[..pos], &input[pos..]),
        None => (input, ""),
    }
}

fn encode_for_url_parse(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 16);
    for ch in input.chars() {
        match ch {
            '{' => out.push_str("%7B"),
            '}' => out.push_str("%7D"),
            '#' => out.push_str("%23"),
            _ => out.push(ch),
        }
    }
    out
}
