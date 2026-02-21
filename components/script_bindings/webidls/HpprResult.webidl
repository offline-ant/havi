/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// Result wrapper for all HpprClient operations
// Provides access to request/response envelope packets alongside the actual result
[Exposed=Window, Pref="dom_hppr_enabled"]
interface HpprResult {
    // The result value - type depends on the operation:
    // - get(): HpprPacket
    // - list(), tips(), headers(), store(), add(): sequence<DOMString>
    // - detach(): undefined
    // - hello(): DOMString
    readonly attribute any value;
    readonly attribute HpprPacket? responseEnvelope;
};
