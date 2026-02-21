/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// HPPR Repo Info Interface
// Only available on havi:// origin through window.ring0.repo
// https://github.com/user/hppr

[Exposed=Window, Pref="dom_hppr_enabled"]
interface HpprRepoInfo {
    // Get the hpprd repo daemon port
    [NewObject] Promise<unsigned short> port();

    // Get the hpprd repo path (e.g., ~/.config/HAVI/repo/)
    [NewObject] Promise<DOMString> repoPath();

    // Get repo status: "embedded" or "external"
    [NewObject] Promise<DOMString> status();
};
