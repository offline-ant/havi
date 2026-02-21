/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR configuration paths for HAVI browser.
//!
//! Config directory resolution:
//! 1. `$HAVI_HOME` (if set)
//! 2. `~/.config/HAVI/` (via `dirs::config_dir()`)
//! 3. `/tmp/HAVI/` (fallback)

use std::path::PathBuf;

/// Root configuration directory for HAVI.
///
/// Desktop: $HAVI_HOME, ~/.config/HAVI/, /tmp/HAVI/
/// Mobile: $HAVI_HOME, /data/local/tmp/HAVI/ (Android), /tmp/HAVI/ (fallback)
pub fn config_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HAVI_HOME") {
        return PathBuf::from(home);
    }

    #[cfg(not(any(target_os = "android", target_env = "ohos")))]
    {
        dirs::config_dir()
            .map(|p| p.join("HAVI"))
            .unwrap_or_else(|| PathBuf::from("/tmp/HAVI"))
    }

    #[cfg(target_os = "android")]
    {
        // Android: use app-specific data directory if available
        PathBuf::from("/data/local/tmp/HAVI")
    }

    #[cfg(target_env = "ohos")]
    {
        // OpenHarmony: use tmp directory
        PathBuf::from("/tmp/HAVI")
    }
}

/// Directory for per-server admin credentials.
/// Path: <config_dir>/credentials/
pub fn credentials_dir() -> PathBuf {
    config_dir().join("credentials")
}

/// Directory for embedded hpprd repository.
/// Path: <config_dir>/repo/
pub fn repo_dir() -> PathBuf {
    config_dir().join("repo")
}

// NOTE: Keep this module scoped to path helpers only. Add higher-level config
// structs only when they're wired up by consumers.
