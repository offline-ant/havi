/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// HPPR Packet representation
// https://github.com/user/hppr

[Exposed=Window, Pref="dom_hppr_enabled"]
interface HpprPacket {
    // Packet identification
    readonly attribute DOMString hash;           // T.B64A.H3 format (48 char)
    readonly attribute DOMString type;           // "Blob", "Plex", "Seal", "Null"

    // Headers access
    DOMString? getHeader(DOMString name);        // Get single header value
    sequence<DOMString> getHeaders(DOMString name);  // Get all values for header
    sequence<DOMString> headers();               // All headers as "Name: value" strings
    sequence<DOMString> customHeaders();           // Custom plex headers only (excludes Group,App, Location, Tai, Blob markline, and Data-Lenght)

    // Coordinate (plex/seal only)
    readonly attribute DOMString? group;
    readonly attribute DOMString? app;
    readonly attribute DOMString? location;
    readonly attribute DOMString? tai;           // TAI timestamp (ssssssssss:ffffffff)
    object? taiDate();                           // TAI as JavaScript Date object
    readonly attribute DOMString? coordinate;    // Full //<group>/<app>/<location>

    // Seal info (seal only)
    readonly attribute DOMString? sealBy;        // V.xxx.H3 verification key

    // Blob Data body access (packet data is fully in memory via Data-Length framing)
    readonly attribute unsigned long long dataLength;
    [Throws, NewObject] ArrayBuffer arrayBuffer();
    [NewObject] Blob blob();
    [Throws] USVString text();
    [Throws] any json();

    // Full raw packet bytes (includes headers)
    [Throws, NewObject] ArrayBuffer raw();
};
