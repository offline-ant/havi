/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::borrow::ToOwned;

use servo_url::BrowserUrl;
use url::quirks;

use crate::dom::bindings::str::USVString;

#[derive(MallocSizeOf)]
pub(crate) struct UrlHelper;

#[expect(non_snake_case)]
impl UrlHelper {
    pub(crate) fn Origin(url: &BrowserUrl) -> USVString {
        USVString(quirks::origin(url.as_url()).to_owned())
    }
    pub(crate) fn Href(url: &BrowserUrl) -> USVString {
        USVString(quirks::href(url.as_url()).to_owned())
    }
    pub(crate) fn Hash(url: &BrowserUrl) -> USVString {
        USVString(quirks::hash(url.as_url()).to_owned())
    }
    pub(crate) fn Host(url: &BrowserUrl) -> USVString {
        USVString(quirks::host(url.as_url()).to_owned())
    }
    pub(crate) fn Port(url: &BrowserUrl) -> USVString {
        USVString(quirks::port(url.as_url()).to_owned())
    }
    pub(crate) fn Search(url: &BrowserUrl) -> USVString {
        USVString(quirks::search(url.as_url()).to_owned())
    }
    pub(crate) fn Hostname(url: &BrowserUrl) -> USVString {
        USVString(quirks::hostname(url.as_url()).to_owned())
    }
    pub(crate) fn Password(url: &BrowserUrl) -> USVString {
        USVString(quirks::password(url.as_url()).to_owned())
    }
    pub(crate) fn Pathname(url: &BrowserUrl) -> USVString {
        USVString(quirks::pathname(url.as_url()).to_owned())
    }
    pub(crate) fn Protocol(url: &BrowserUrl) -> USVString {
        USVString(quirks::protocol(url.as_url()).to_owned())
    }
    pub(crate) fn Username(url: &BrowserUrl) -> USVString {
        USVString(quirks::username(url.as_url()).to_owned())
    }
    pub(crate) fn SetHash(url: &mut BrowserUrl, value: USVString) {
        quirks::set_hash(url.as_mut_url(), &value.0)
    }
    pub(crate) fn SetHost(url: &mut BrowserUrl, value: USVString) {
        let _ = quirks::set_host(url.as_mut_url(), &value.0);
    }
    pub(crate) fn SetPort(url: &mut BrowserUrl, value: USVString) {
        let _ = quirks::set_port(url.as_mut_url(), &value.0);
    }
    pub(crate) fn SetSearch(url: &mut BrowserUrl, value: USVString) {
        quirks::set_search(url.as_mut_url(), &value.0)
    }
    pub(crate) fn SetPathname(url: &mut BrowserUrl, value: USVString) {
        quirks::set_pathname(url.as_mut_url(), &value.0)
    }
    pub(crate) fn SetHostname(url: &mut BrowserUrl, value: USVString) {
        let _ = quirks::set_hostname(url.as_mut_url(), &value.0);
    }
    pub(crate) fn SetPassword(url: &mut BrowserUrl, value: USVString) {
        let _ = quirks::set_password(url.as_mut_url(), &value.0);
    }
    pub(crate) fn SetProtocol(url: &mut BrowserUrl, value: USVString) {
        let _ = quirks::set_protocol(url.as_mut_url(), &value.0);
    }
    pub(crate) fn SetUsername(url: &mut BrowserUrl, value: USVString) {
        let _ = quirks::set_username(url.as_mut_url(), &value.0);
    }
}
