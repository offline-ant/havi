/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// Address (HPPR Address) for HPPR
// Combines scheme and URC: scheme://group/app/location{via:endpoint}
// https://github.com/user/hppr

[Exposed=Window, Pref="dom_hppr_enabled"]
interface Address {
    [Throws] constructor(USVString input);

    [Throws] stringifier attribute USVString href;
    [Throws]          attribute DOMString scheme;  // "hppr", "hppr-setup", "hppr-sandbox", "hppr-browse", "hppr-editor"
    [Throws]          attribute DOMString? endpoint;   // "host:port" or null
    readonly          attribute DOMString? coordinate; // "//group/app/location" string

    [SameObject] readonly attribute URC urc;

    // Convenience setters trigger navigation (like window.location setters)
    [Throws] attribute DOMString? group;
    [Throws] attribute DOMString? app;
    [Throws] attribute DOMString? location;
    readonly attribute boolean isListing;
    readonly attribute boolean hasDirectEndpoint;

    // Query/fragment metadata (JSONqa) - delegates to urc
    [Throws] attribute object? qa;
    readonly attribute DOMString? fragment;
};
