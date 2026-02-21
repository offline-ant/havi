/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// HPPR Protocol Error - extends DOMException with HPPR-specific properties

[Exposed=Window, Pref="dom_hppr_enabled"]
interface HpprError : DOMException {
    [Throws] constructor(DOMString errorType, optional DOMString detail = "");

    // HPPR error type (NOT_FOUND, FORBIDDEN, CONNECTION, etc.)
    readonly attribute DOMString type;

    // Human-readable detail message
    readonly attribute DOMString detail;

    // Whether this error closed the connection (FATAL responses, connection errors)
    readonly attribute boolean fatal;
};
