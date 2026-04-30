/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR configuration paths for HAVI browser.
//!
//! Config directory resolution:
//! 1. `$HAVI_CONFIG` (if set)
//! 2. `~/.config/HAVI/` (via `dirs::config_dir()`)
//! 3. `/tmp/HAVI/` (fallback)
//!
//! `HAVI_HOME` is reserved for hpprd endpoint specification (not a path).
//! Browser-local non-repo state lives separately in `havi.sqlite`.

use std::path::PathBuf;

/// Root configuration directory for HAVI.
///
/// Desktop: $HAVI_CONFIG, ~/.config/HAVI/, /tmp/HAVI/
/// Mobile: $HAVI_CONFIG, /data/local/tmp/HAVI/ (Android), /tmp/HAVI/ (fallback)
pub fn config_dir() -> PathBuf {
    if let Ok(path) = std::env::var("HAVI_CONFIG") {
        return PathBuf::from(path);
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

/// SQLite state DB file.
/// Path: <config_dir>/havi.sqlite
pub fn db_path() -> PathBuf {
    config_dir().join("havi.sqlite")
}

/// Single-instance IPC socket path (Unix).
/// Path: <config_dir>/havi.sock
pub fn ipc_socket_path() -> PathBuf {
    config_dir().join("havi.sock")
}

/// Browser-owned packet-store database.
/// Path: <config_dir>/havi-packets.sqlite
pub fn packet_store_path() -> PathBuf {
    config_dir().join("havi-packets.sqlite")
}


// NOTE: Keep this module scoped to path helpers only. Add higher-level config
// structs only when they're wired up by consumers.
