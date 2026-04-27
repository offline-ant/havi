/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// HAVI internal helper capability root.
// Only available on HAVI-owned internal helper pages through window.havi.

[Exposed=Window, Pref="dom_hppr_enabled"]
interface HaviInternal {
    readonly attribute HaviAdmin? admin;
};
