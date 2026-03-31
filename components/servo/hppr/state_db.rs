/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! SQLite-backed HAVI local state.
//!
//! Stores HAVI-local configuration and history in `<config_dir>/havi.sqlite`.

#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowKeyEntry {
    pub group: String,
    pub app: String,
    pub signing_key: String,
    pub verification_key: String,
}

use super::config;

/// Shared state DB handle.
pub type StateDbHandle = Arc<StateDb>;

/// One history row.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: i64,
    pub ts_unix: i64,
    pub url: String,
    pub title: String,
}

/// SQLite-backed local state.
pub struct StateDb {
    conn: Mutex<Connection>,
}

impl StateDb {
    /// Open or create a state DB at the given path and initialize schema.
    pub fn open(path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create db parent '{}': {}", parent.display(), e))?;
        }

        let conn = Connection::open(&path)
            .map_err(|e| format!("failed to open sqlite db '{}': {}", path.display(), e))?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS meta (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS settings (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS credentials (
                 repo_vkey TEXT PRIMARY KEY,
                 ring1_name TEXT NOT NULL,
                 token TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS history (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 ts_unix INTEGER NOT NULL,
                 url TEXT NOT NULL,
                 title TEXT NOT NULL DEFAULT ''
             );
             CREATE TABLE IF NOT EXISTS shadow_keys (
                 group_name TEXT NOT NULL,
                 app_name TEXT NOT NULL,
                 signing_key TEXT NOT NULL,
                 verification_key TEXT NOT NULL,
                 PRIMARY KEY (group_name, app_name)
             );
             CREATE TABLE IF NOT EXISTS shadow_overrides (
                 group_name TEXT NOT NULL,
                 app_name TEXT NOT NULL,
                 enabled INTEGER NOT NULL,
                 PRIMARY KEY (group_name, app_name)
             );
             CREATE INDEX IF NOT EXISTS idx_history_ts ON history(ts_unix DESC);
             INSERT INTO meta(key, value)
                 VALUES ('schema_version', '2')
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value;",
        )
        .map_err(|e| format!("failed to initialize sqlite schema: {}", e))?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open the default HAVI state DB.
    pub fn open_default() -> Result<Self, String> {
        Self::open(config::db_path())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|_| "db mutex poisoned")?;
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| format!("failed to read setting '{}': {}", key, e))
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|_| "db mutex poisoned")?;
        conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )
        .map_err(|e| format!("failed to write setting '{}': {}", key, e))?;
        Ok(())
    }

    pub fn get_credential(&self, repo_vkey: &str) -> Result<Option<(String, String)>, String> {
        let conn = self.conn.lock().map_err(|_| "db mutex poisoned")?;
        conn.query_row(
            "SELECT ring1_name, token FROM credentials WHERE repo_vkey = ?1",
            params![repo_vkey],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|e| format!("failed to read credential '{}': {}", repo_vkey, e))
    }

    pub fn set_credential(
        &self,
        repo_vkey: &str,
        ring1_name: &str,
        token: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|_| "db mutex poisoned")?;
        conn.execute(
            "INSERT INTO credentials(repo_vkey, ring1_name, token) VALUES (?1, ?2, ?3)
             ON CONFLICT(repo_vkey) DO UPDATE
             SET ring1_name=excluded.ring1_name, token=excluded.token",
            params![repo_vkey, ring1_name, token],
        )
        .map_err(|e| format!("failed to write credential '{}': {}", repo_vkey, e))?;
        Ok(())
    }

    pub fn insert_history(&self, url: &str, title: &str) -> Result<(), String> {
        let ts_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let conn = self.conn.lock().map_err(|_| "db mutex poisoned")?;
        conn.execute(
            "INSERT INTO history(ts_unix, url, title) VALUES (?1, ?2, ?3)",
            params![ts_unix, url, title],
        )
        .map_err(|e| format!("failed to write history row: {}", e))?;
        Ok(())
    }

    pub fn get_shadow_key(&self, group: &str, app: &str) -> Result<Option<ShadowKeyEntry>, String> {
        let conn = self.conn.lock().map_err(|_| "db mutex poisoned")?;
        conn.query_row(
            "SELECT signing_key, verification_key
             FROM shadow_keys
             WHERE group_name = ?1 AND app_name = ?2",
            params![group, app],
            |row| {
                Ok(ShadowKeyEntry {
                    group: group.to_string(),
                    app: app.to_string(),
                    signing_key: row.get(0)?,
                    verification_key: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(|e| format!("failed to read shadow key for {}/{}: {}", group, app, e))
    }

    pub fn set_shadow_key(
        &self,
        group: &str,
        app: &str,
        signing_key: &str,
        verification_key: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|_| "db mutex poisoned")?;
        conn.execute(
            "INSERT INTO shadow_keys(group_name, app_name, signing_key, verification_key)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(group_name, app_name) DO UPDATE
             SET signing_key = excluded.signing_key,
                 verification_key = excluded.verification_key",
            params![group, app, signing_key, verification_key],
        )
        .map_err(|e| format!("failed to write shadow key for {}/{}: {}", group, app, e))?;
        Ok(())
    }

    pub fn shadow_override_enabled(&self, group: &str, app: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|_| "db mutex poisoned")?;
        let enabled = conn
            .query_row(
                "SELECT enabled
                 FROM shadow_overrides
                 WHERE group_name = ?1 AND app_name = ?2",
                params![group, app],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|e| format!("failed to read shadow override for {}/{}: {}", group, app, e))?;
        Ok(enabled.unwrap_or(0) != 0)
    }

    pub fn set_shadow_override(&self, group: &str, app: &str, enabled: bool) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|_| "db mutex poisoned")?;
        conn.execute(
            "INSERT INTO shadow_overrides(group_name, app_name, enabled)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(group_name, app_name) DO UPDATE
             SET enabled = excluded.enabled",
            params![group, app, if enabled { 1 } else { 0 }],
        )
        .map_err(|e| format!("failed to write shadow override for {}/{}: {}", group, app, e))?;
        Ok(())
    }

    pub fn list_history(&self, limit: usize) -> Result<Vec<HistoryEntry>, String> {
        let conn = self.conn.lock().map_err(|_| "db mutex poisoned")?;
        let mut stmt = conn
            .prepare(
                "SELECT id, ts_unix, url, title
                 FROM history
                 ORDER BY ts_unix DESC, id DESC
                 LIMIT ?1",
            )
            .map_err(|e| format!("failed to prepare history query: {}", e))?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    ts_unix: row.get(1)?,
                    url: row.get(2)?,
                    title: row.get(3)?,
                })
            })
            .map_err(|e| format!("failed to query history: {}", e))?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| format!("failed to decode history row: {}", e))?);
        }
        Ok(out)
    }
}

static GLOBAL_STATE_DB: OnceLock<StateDbHandle> = OnceLock::new();

/// Global SQLite state DB singleton.
pub fn global_state_db() -> StateDbHandle {
    GLOBAL_STATE_DB
        .get_or_init(|| {
            Arc::new(
                StateDb::open_default()
                    .unwrap_or_else(|e| panic!("failed to initialize HAVI sqlite state db: {}", e)),
            )
        })
        .clone()
}

/// Helper for tests: create a standalone DB handle at a temp path.
#[cfg(test)]
pub fn open_test_db(path: &Path) -> StateDbHandle {
    Arc::new(StateDb::open(path.to_path_buf()).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_roundtrip() {
        let db_path = std::env::temp_dir().join("havi_state_db_credentials_roundtrip.sqlite");
        let _ = std::fs::remove_file(&db_path);
        let db = StateDb::open(db_path.clone()).unwrap();

        db.set_credential("V.test.H3", "ring0", "init").unwrap();
        let cred = db.get_credential("V.test.H3").unwrap();
        assert_eq!(cred, Some(("ring0".to_string(), "init".to_string())));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn history_roundtrip() {
        let db_path = std::env::temp_dir().join("havi_state_db_history_roundtrip.sqlite");
        let _ = std::fs::remove_file(&db_path);
        let db = StateDb::open(db_path.clone()).unwrap();

        db.insert_history("hppr://u/web/index.html", "index.html")
            .unwrap();
        let rows = db.list_history(10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].url, "hppr://u/web/index.html");

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn shadow_key_roundtrip() {
        let db_path = std::env::temp_dir().join("havi_state_db_shadow_key_roundtrip.sqlite");
        let _ = std::fs::remove_file(&db_path);
        let db = StateDb::open(db_path.clone()).unwrap();

        db.set_shadow_key("dev", "hppr.forge", "&.shadow.H3", "V.shadow.H3")
            .unwrap();
        let row = db.get_shadow_key("dev", "hppr.forge").unwrap().unwrap();
        assert_eq!(row.group, "dev");
        assert_eq!(row.app, "hppr.forge");
        assert_eq!(row.signing_key, "&.shadow.H3");
        assert_eq!(row.verification_key, "V.shadow.H3");

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn shadow_override_roundtrip() {
        let db_path = std::env::temp_dir().join("havi_state_db_shadow_override_roundtrip.sqlite");
        let _ = std::fs::remove_file(&db_path);
        let db = StateDb::open(db_path.clone()).unwrap();

        assert!(!db.shadow_override_enabled("dev", "hppr.forge").unwrap());
        db.set_shadow_override("dev", "hppr.forge", true).unwrap();
        assert!(db.shadow_override_enabled("dev", "hppr.forge").unwrap());
        db.set_shadow_override("dev", "hppr.forge", false).unwrap();
        assert!(!db.shadow_override_enabled("dev", "hppr.forge").unwrap());

        let _ = std::fs::remove_file(db_path);
    }
}
