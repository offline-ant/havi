/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

[Exposed=Window, Pref="dom_hppr_enabled"]
interface WindowAddress {
    [Throws] stringifier attribute USVString href;
    readonly attribute DOMString scheme;
    [Throws] readonly attribute object? qa;
    readonly attribute DOMString? fragment;
    readonly attribute boolean isListing;
};
