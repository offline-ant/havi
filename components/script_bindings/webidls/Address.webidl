/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// Detached parsed HPPR-family exact address value.
// window.address uses browser-owned WindowAddress surfaces instead.

[Exposed=Window, Pref="dom_hppr_enabled"]
interface Address {
    [Throws] constructor(USVString input);

    [Throws] stringifier attribute USVString href;
    readonly attribute DOMString scheme;
    readonly attribute DOMString? coordinate;

    [SameObject] readonly attribute URC urc;

    [Throws] attribute DOMString? group;
    [Throws] attribute DOMString? api;
    [Throws] attribute DOMString? key;
    readonly attribute boolean isListing;

    [Throws] attribute object? qa;
    readonly attribute DOMString? fragment;
};
