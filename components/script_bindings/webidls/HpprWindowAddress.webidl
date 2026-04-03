/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

[Exposed=Window, Pref="dom_hppr_enabled"]
interface HpprWindowAddress : WindowAddress {
    readonly attribute DOMString? coordinate;
    [SameObject] readonly attribute URC urc;

    [Throws] attribute DOMString? group;
    [Throws] attribute DOMString? app;
    [Throws] attribute DOMString? location;
};
