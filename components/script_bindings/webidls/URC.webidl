/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// URC (Unified Resource Coordinate) for HPPR
// https://github.com/user/hppr

[Exposed=Window, Pref="dom_hppr_enabled"]
interface URC {
    [Throws] constructor(USVString input);

    stringifier readonly attribute USVString href;
    readonly attribute DOMString method;  // "hash" or "index"

    readonly attribute DOMString? group;
    readonly attribute DOMString? app;
    readonly attribute DOMString? location;
    readonly attribute DOMString? coordinate;

    readonly attribute boolean isListing;

    URCSelector? getSelector();

    // Query/fragment metadata (JSONqa)
    [Throws] attribute object? qa;
    readonly attribute DOMString? fragment;

    [Throws] URC join(USVString coordinate);
    URC setListing(boolean isListing);
};

dictionary URCSelector {
    DOMString type;           // "empty", "plex", or "seal"
    DOMString? verifyingKey;
    DOMString? tai;
    DOMString? hash;
};
