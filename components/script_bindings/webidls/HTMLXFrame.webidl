/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// HTMLXFrame - native iframe replacement for HPPR content (<x src="...">)

[Exposed=Window, Pref="dom_hppr_enabled"]
interface HTMLXFrame : HTMLElement {
    [HTMLConstructor] constructor();

    // Source attribute - URC or relative coordinate
    [CEReactions] attribute USVString src;

    // Trust inheritance from parent document's site-trust
    [CEReactions] attribute boolean trustParent;

    // Watch for coordinate changes
    [CEReactions] attribute DOMString watch;

    // Sizing
    [CEReactions] attribute DOMString width;
    [CEReactions] attribute DOMString height;

    // Content access
    readonly attribute HpprPacket? packet;
    readonly attribute Document? contentDocument;
    readonly attribute WindowProxy? contentWindow;
};
