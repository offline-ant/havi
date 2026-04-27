/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// HAVI internal admin capability descriptor.
// Exposes explicit helper-page repo-admin power. Not part of ordinary page APIs.

[Exposed=Window, Pref="dom_hppr_enabled"]
interface HaviAdmin {
    [SameObject] readonly attribute HpprClient client;
    [SameObject] readonly attribute HpprRepoInfo repo;
};
