/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![deny(unsafe_code)]
#![crate_name = "servo_url"]
#![crate_type = "rlib"]

pub mod encoding;
pub mod hppr;
pub mod origin;

use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::Hasher;
use std::net::IpAddr;
use std::ops::{Index, Range, RangeFrom, RangeFull, RangeTo};
use std::path::Path;
use std::str::FromStr;

use hppr_packet::urc::URC;
use malloc_size_of_derive::MallocSizeOf;
use serde::{Deserialize, Serialize};
use servo_arc::Arc;
pub use url::Host;
use url::{Position, Url};
use uuid::Uuid;

pub use crate::origin::{ImmutableOrigin, MutableOrigin, OpaqueOrigin, OriginSnapshot};
pub use hppr::{Endpoint, HAVIAddress, HaviUrl, HpprScheme, HpprUrl, HpprUrlParseError, via_url};
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

#[derive(Clone, Deserialize, Eq, Hash, MallocSizeOf, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ServoUrl(#[conditional_malloc_size_of] Arc<Url>);

impl ServoUrl {
    pub fn from_url(url: Url) -> Self {
        ServoUrl(Arc::new(url))
    }

    pub fn parse_with_base(base: Option<&Self>, input: &str) -> Result<Self, url::ParseError> {
        Url::options()
            .base_url(base.map(|b| &*b.0))
            .parse(input)
            .map(Self::from_url)
    }

    pub fn into_string(self) -> String {
        String::from(self.into_url())
    }

    pub fn into_url(self) -> Url {
        self.as_url().clone()
    }

    pub fn get_arc(&self) -> Arc<Url> {
        self.0.clone()
    }

    pub fn as_url(&self) -> &Url {
        &self.0
    }

    pub fn hosturc(&self) -> Option<HAVIAddress> {
        HAVIAddress::parse(self.as_str()).ok()
    }

    pub fn parse(input: &str) -> Result<Self, url::ParseError> {
        Url::parse(input).map(Self::from_url)
    }

    pub fn cannot_be_a_base(&self) -> bool {
        self.0.cannot_be_a_base()
    }

    pub fn domain(&self) -> Option<&str> {
        self.0.domain()
    }

    pub fn fragment(&self) -> Option<&str> {
        self.0.fragment()
    }

    pub fn path(&self) -> &str {
        self.0.path()
    }

    pub fn origin(&self) -> ImmutableOrigin {
        match HpprUrl::parse(self.as_str()) {
            Ok(HpprUrl::HAVIAddress(address)) => {
                hppr_origin_for_address(&address).unwrap_or_else(|| ImmutableOrigin::new_opaque())
            },
            Ok(HpprUrl::Havi(_)) => ImmutableOrigin::new_opaque(),
            Err(_) => ImmutableOrigin::new(self.0.origin()),
        }
    }

    pub fn scheme(&self) -> &str {
        self.0.scheme()
    }

    pub fn is_secure_scheme(&self) -> bool {
        let scheme = self.scheme();
        scheme == "https" || scheme == "wss"
    }

    /// <https://fetch.spec.whatwg.org/#local-scheme>
    pub fn is_local_scheme(&self) -> bool {
        let scheme = self.scheme();
        scheme == "about" || scheme == "blob" || scheme == "data"
    }

    /// <https://url.spec.whatwg.org/#special-scheme>
    pub fn is_special_scheme(&self) -> bool {
        let scheme = self.scheme();
        scheme == "ftp" ||
            scheme == "file" ||
            scheme == "http" ||
            scheme == "https" ||
            scheme == "ws" ||
            scheme == "wss"
    }

    /// <https://url.spec.whatwg.org/#url-equivalence>
    /// In the future this may be removed if the helper is added upstream in rust-url
    /// see <https://github.com/servo/rust-url/issues/1063> for details
    pub fn is_equal_excluding_fragments(&self, other: &ServoUrl) -> bool {
        self.0[..Position::AfterQuery] == other.0[..Position::AfterQuery]
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Display URL with decoded JSONqa for HPPR URLs.
    ///
    /// Returns the URL string with `%7B`/`%7D`/`%23` decoded back to
    /// `{`/`}`/`#` inside JSONqa suffixes. For non-HPPR URLs, returns
    /// the standard string representation.
    pub fn hppr_display_url(&self) -> String {
        let s = self.0.as_str();
        if s.starts_with("hppr:") || s.starts_with("hppr-") {
            hppr::percent_decode_jsonqa(s)
        } else {
            s.to_owned()
        }
    }

    pub fn as_mut_url(&mut self) -> &mut Url {
        Arc::make_mut(&mut self.0)
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
        self.0.username()
    }

    pub fn password(&self) -> Option<&str> {
        self.0.password()
    }

    pub fn to_file_path(&self) -> Result<::std::path::PathBuf, UrlError> {
        self.0.to_file_path().map_err(|_| UrlError::ToFilePath)
    }

    pub fn host(&self) -> Option<url::Host<&str>> {
        self.0.host()
    }

    pub fn host_str(&self) -> Option<&str> {
        self.0.host_str()
    }

    pub fn port(&self) -> Option<u16> {
        self.0.port()
    }

    pub fn port_or_known_default(&self) -> Option<u16> {
        self.0.port_or_known_default()
    }

    pub fn join(&self, input: &str) -> Result<ServoUrl, url::ParseError> {
        if let Ok(HpprUrl::HAVIAddress(address)) = HpprUrl::parse(self.as_str()) {
            return self.join_hppr(input, &address);
        }
        self.0.join(input).map(Self::from_url)
    }

    /// Join relative URL using URC resolution for HPPR coordinates.
    ///
    /// Coordinate-relative joins follow HPPR coordinate structure (`//group/app/location`),
    /// not HTTP path semantics. The base coordinate's "file" segment is stripped before
    /// resolving, so `//chess/game/board.html` + `style.css` = `//chess/game/style.css`.
    /// Absolute coordinates (`//other/app/...`) replace the entire coordinate.
    fn join_hppr(&self, input: &str, address: &HAVIAddress) -> Result<ServoUrl, url::ParseError> {
        // Strip JSONqa suffix ({...}) before URL resolution.
        // JSONqa is client-side metadata excluded from coordinate lookup.
        // Characters like { } # inside JSONqa would be mangled by Url::parse.
        let (coord_input, jsonqa) = split_jsonqa(input);

        // Absolute URL with scheme - parse directly
        if let Some(colon) = coord_input.find(':') {
            if colon > 0 &&
                !coord_input.starts_with('.') &&
                !coord_input.starts_with('/') &&
                coord_input[..colon]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                if let Ok(url) = Url::parse(coord_input) {
                    return Ok(Self::from_url(url));
                }
            }
        }

        // Pure JSONqa with no coordinate (e.g. "{#:text}") — same-document qualifier.
        // Resolve against current document's full coordinate.
        if coord_input.is_empty() {
            let url_str = format!(
                "{}{}",
                address.reconstruct(&address.urc_string()),
                encode_jsonqa_for_url(jsonqa),
            );
            return Url::parse(&url_str).map(Self::from_url);
        }

        // Absolute coordinate (//group/app/loc)
        if coord_input.starts_with("//") {
            let url_str = format!(
                "{}{}",
                address.reconstruct(coord_input),
                encode_jsonqa_for_url(jsonqa),
            );
            return Url::parse(&url_str).map(Self::from_url);
        }

        // For web-like resolution: if coordinate doesn't end with '/', strip the "file" segment.
        // e.g., //chess/game/board.html -> //chess/game/ before joining with style.css
        let urc_string = address.urc_string();
        let base_coord = if urc_string.ends_with('/') {
            urc_string
        } else {
            match urc_string.rfind('/') {
                Some(pos) => format!("{}/", &urc_string[..pos]),
                None => urc_string,
            }
        };

        // Parse base coordinate as URC and use URC::join()
        let Ok(current) = URC::parse(base_coord) else {
            return self.0.join(input).map(Self::from_url);
        };

        let Ok(resolved) = current.join(coord_input) else {
            return self.0.join(input).map(Self::from_url);
        };

        let url_str = format!(
            "{}{}",
            address.reconstruct(&resolved.unwrap()),
            encode_jsonqa_for_url(jsonqa),
        );
        Url::parse(&url_str).map(Self::from_url)
    }

    pub fn path_segments(&self) -> Option<::std::str::Split<'_, char>> {
        self.0.path_segments()
    }

    pub fn query(&self) -> Option<&str> {
        self.0.query()
    }

    pub fn from_file_path<P: AsRef<Path>>(path: P) -> Result<Self, UrlError> {
        Url::from_file_path(path)
            .map(Self::from_url)
            .map_err(|_| UrlError::FromFilePath)
    }

    /// Return a non-standard shortened form of the URL. Mainly intended to be
    /// used for debug printing in a constrained space (e.g., thread names).
    pub fn debug_compact(&self) -> impl std::fmt::Display + '_ {
        match self.scheme() {
            "http" | "https" => {
                // Strip `scheme://`, which is hardly useful for identifying websites
                let mut st = self.as_str();
                st = st.strip_prefix(self.scheme()).unwrap_or(st);
                st = st.strip_prefix(':').unwrap_or(st);
                st = st.trim_start_matches('/');

                // Don't want to return an empty string
                if st.is_empty() {
                    st = self.as_str();
                }

                st
            },
            "file" => {
                // The only useful part in a `file` URL is usually only the last
                // few components
                let path = self.path();
                let i = path.rfind('/');
                let i = i.map(|i| path[..i].rfind('/').unwrap_or(i));
                match i {
                    None | Some(0) => path,
                    Some(i) => &path[i + 1..],
                }
            },
            _ => self.as_str(),
        }
    }

    /// <https://w3c.github.io/webappsec-secure-contexts/#potentially-trustworthy-url>
    pub fn is_potentially_trustworthy(&self) -> bool {
        // Step 1
        if self.as_str() == "about:blank" || self.as_str() == "about:srcdoc" {
            return true;
        }
        // Step 2
        if self.scheme() == "data" {
            return true;
        }
        // Step 3
        self.origin().is_potentially_trustworthy()
    }

    /// <https://html.spec.whatwg.org/multipage/#matches-about:blank>
    pub fn matches_about_blank(&self) -> bool {
        // A URL matches about:blank if

        // its scheme is "about",
        let scheme_is_about = self.scheme() == "about";

        // its path contains a single string "blank",
        let path_is_blank = self.0.path() == "blank";

        // its username and password are the empty string,
        let empty_username_and_password =
            self.0.username().is_empty() && self.0.password().is_none();

        // and its host is null.
        let null_host = self.0.host().is_none();

        scheme_is_about && path_is_blank && empty_username_and_password && null_host
    }
}

impl fmt::Display for ServoUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl fmt::Debug for ServoUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let url_string = self.0.as_str();
        if self.scheme() != "data" || url_string.len() <= DATA_URL_DISPLAY_LENGTH {
            return url_string.fmt(formatter);
        }

        let mut hasher = DefaultHasher::new();
        hasher.write(self.0.as_str().as_bytes());

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

impl Index<RangeFull> for ServoUrl {
    type Output = str;
    fn index(&self, _: RangeFull) -> &str {
        &self.0[..]
    }
}

impl Index<RangeFrom<Position>> for ServoUrl {
    type Output = str;
    fn index(&self, range: RangeFrom<Position>) -> &str {
        &self.0[range]
    }
}

impl Index<RangeTo<Position>> for ServoUrl {
    type Output = str;
    fn index(&self, range: RangeTo<Position>) -> &str {
        &self.0[range]
    }
}

impl Index<Range<Position>> for ServoUrl {
    type Output = str;
    fn index(&self, range: Range<Position>) -> &str {
        &self.0[range]
    }
}

impl From<Url> for ServoUrl {
    fn from(url: Url) -> Self {
        ServoUrl::from_url(url)
    }
}

impl From<Arc<Url>> for ServoUrl {
    fn from(url: Arc<Url>) -> Self {
        ServoUrl(url)
    }
}

impl FromStr for ServoUrl {
    type Err = <Url as FromStr>::Err;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let url = Url::from_str(value)?;
        Ok(url.into())
    }
}

/// Split JSONqa suffix from a URL or href string.
///
/// Returns (coordinate_part, jsonqa_part). The jsonqa_part includes
/// the outer braces (e.g. `{#:text,page:5}`), or is empty if no JSONqa.
fn split_jsonqa(input: &str) -> (&str, &str) {
    // Find opening brace (literal or percent-encoded)
    let brace_pos = input.find('{').or_else(|| input.find("%7B").or_else(|| input.find("%7b")));
    match brace_pos {
        Some(pos) => (&input[..pos], &input[pos..]),
        None => (input, ""),
    }
}

/// Percent-encode JSONqa for safe passage through rust-url's Url::parse.
///
/// Encodes `{`, `}`, `#` and other URL-special characters so they survive
/// Url::parse without being interpreted as URL structure. HAVIAddress::parse
/// decodes these back when parsing the URL.
fn encode_jsonqa_for_url(jsonqa: &str) -> String {
    if jsonqa.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(jsonqa.len() * 2);
    for ch in jsonqa.chars() {
        match ch {
            '{' => out.push_str("%7B"),
            '}' => out.push_str("%7D"),
            '#' => out.push_str("%23"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hppr_routed_join_relative() {
        let base = ServoUrl::parse("hppr://chess/game/board.html").unwrap();
        assert_eq!(
            base.join("style.css").unwrap().as_str(),
            "hppr://chess/game/style.css"
        );
    }

    #[test]
    fn hppr_via_join_relative() {
        // Relative join does NOT preserve {via:...}
        let base = ServoUrl::parse("hppr://chess/game/board.html%7Bvia:192.168.1.10:4777%7D").unwrap();
        assert_eq!(
            base.join("style.css").unwrap().as_str(),
            "hppr://chess/game/style.css"
        );
    }

    #[test]
    fn hppr_join_parent() {
        let base = ServoUrl::parse("hppr://g/a/sub/file.html%7Bvia:10.0.0.1:4777%7D").unwrap();
        assert_eq!(
            base.join("../other.html").unwrap().as_str(),
            "hppr://g/a/other.html"
        );
    }

    #[test]
    fn hppr_join_absolute_coord() {
        // Absolute coordinate join does NOT preserve {via:...}
        let base = ServoUrl::parse("hppr://g/a/file.html%7Bvia:10.0.0.1:4777%7D").unwrap();
        assert_eq!(
            base.join("//other/app/index.html").unwrap().as_str(),
            "hppr://other/app/index.html"
        );
    }

    #[test]
    fn hppr_sandbox_join() {
        // Relative join does NOT preserve {via:...}
        let base = ServoUrl::parse("hppr-sandbox://g/app/index.html%7Bvia:10.0.0.5:4778%7D").unwrap();
        assert_eq!(
            base.join("style.css").unwrap().as_str(),
            "hppr-sandbox://g/app/style.css"
        );
    }

    #[test]
    fn hppr_origin_isolated_by_app() {
        let app1 = ServoUrl::parse("hppr://g1/app1/index.html").unwrap().origin();
        let app2 = ServoUrl::parse("hppr://g1/app2/index.html").unwrap().origin();
        let app1_other = ServoUrl::parse("hppr://g1/app1/other.html").unwrap().origin();

        assert_ne!(app1, app2);
        assert_eq!(app1, app1_other);
    }

    #[test]
    fn hppr_origin_ignores_endpoint() {
        let routed = ServoUrl::parse("hppr://g1/app1/index.html").unwrap().origin();
        let direct = ServoUrl::parse("hppr://g1/app1/index.html%7Bvia:10.0.0.1:4777%7D")
            .unwrap()
            .origin();

        assert_eq!(routed, direct);
    }

    #[test]
    fn jsonqa_same_document() {
        // Pure JSONqa href stays on current document
        let base = ServoUrl::parse("hppr://u/web/index.html").unwrap();
        let result = base.join("{#:text}").unwrap();
        assert_eq!(result.hppr_display_url(), "hppr://u/web/index.html{#:text}");
    }

    #[test]
    fn jsonqa_with_page() {
        let base = ServoUrl::parse("hppr://docs/manual/chapter-3").unwrap();
        let result = base.join("{page:5}").unwrap();
        assert_eq!(result.hppr_display_url(), "hppr://docs/manual/chapter-3{page:5}");
    }

    #[test]
    fn jsonqa_relative_with_qa() {
        let base = ServoUrl::parse("hppr://g/a/dir/page.html").unwrap();
        let result = base.join("other.html{#:section}").unwrap();
        assert_eq!(result.hppr_display_url(), "hppr://g/a/dir/other.html{#:section}");
    }

    #[test]
    fn jsonqa_absolute_coord_with_qa() {
        let base = ServoUrl::parse("hppr://g/a/page.html").unwrap();
        let result = base.join("//other/app/index.html{page:1}").unwrap();
        assert_eq!(result.hppr_display_url(), "hppr://other/app/index.html{page:1}");
    }

    #[test]
    fn jsonqa_hash_not_fragment() {
        // # inside JSONqa must not be treated as fragment separator
        let base = ServoUrl::parse("hppr://u/web/index.html").unwrap();
        let result = base.join("{#:results}").unwrap();
        let display = result.hppr_display_url();
        assert!(display.contains("{#:results}"), "got: {}", display);
        // The # must not appear as a URL fragment
        assert!(!result.as_str().contains('#'), "raw URL has fragment: {}", result.as_str());
    }
}
