use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub struct DbState(pub Mutex<Connection>);

/// One row of durable user memory. `kind` is either:
/// - "rule":    freeform guidance consumed by the LLM (key = NULL)
/// - "setting": exact key-value consumed by Rust code (e.g. transcript_dir)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryItem {
    pub id: i64,
    pub kind: String,
    pub key: Option<String>,
    pub value: String,
    pub created_at: String,
}

pub fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            kind       TEXT NOT NULL CHECK (kind IN ('rule','setting')),
            key        TEXT,
            value      TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_setting_key
            ON memory(key) WHERE kind = 'setting';",
    )?;
    migrate_legacy_tables(conn)?;
    Ok(())
}

/// One-time migration from the old split `preferences` / `settings` tables.
fn migrate_legacy_tables(conn: &Connection) -> rusqlite::Result<()> {
    let has_table = |name: &str| -> rusqlite::Result<bool> {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
    };

    if has_table("preferences")? {
        conn.execute_batch(
            "INSERT INTO memory (kind, value, created_at)
                 SELECT 'rule', rule, created_at FROM preferences;
             DROP TABLE preferences;",
        )?;
    }
    if has_table("settings")? {
        conn.execute_batch(
            "INSERT INTO memory (kind, key, value)
                 SELECT 'setting', key, value FROM settings;
             DROP TABLE settings;",
        )?;
    }
    Ok(())
}

fn row_to_item(row: &rusqlite::Row) -> rusqlite::Result<MemoryItem> {
    Ok(MemoryItem {
        id: row.get(0)?,
        kind: row.get(1)?,
        key: row.get(2)?,
        value: row.get(3)?,
        created_at: row.get(4)?,
    })
}

/// Internal helper for Rust-side reads (e.g. transcript save settings).
pub fn get_setting_value(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM memory WHERE kind = 'setting' AND key = ?1",
        params![key],
        |row| row.get(0),
    )
    .ok()
}

#[tauri::command]
pub fn store_preference(
    rule: String,
    state: tauri::State<'_, DbState>,
) -> Result<MemoryItem, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO memory (kind, value) VALUES ('rule', ?1)",
        params![rule],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    conn.query_row(
        "SELECT id, kind, key, value, created_at FROM memory WHERE id = ?1",
        params![id],
        row_to_item,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_setting(key: String, state: tauri::State<'_, DbState>) -> Result<Option<String>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(get_setting_value(&conn, &key))
}

#[tauri::command]
pub fn set_setting(
    key: String,
    value: String,
    state: tauri::State<'_, DbState>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM memory WHERE kind = 'setting' AND key = ?1",
        params![key],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO memory (kind, key, value) VALUES ('setting', ?1, ?2)",
        params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_memory(state: tauri::State<'_, DbState>) -> Result<Vec<MemoryItem>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, kind, key, value, created_at FROM memory
             ORDER BY kind = 'setting', created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], row_to_item)
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(items)
}

#[tauri::command]
pub fn delete_memory(id: i64, state: tauri::State<'_, DbState>) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM memory WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
