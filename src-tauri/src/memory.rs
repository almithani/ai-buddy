use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub struct DbState(pub Mutex<Connection>);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Preference {
    pub id: i64,
    pub rule: String,
    pub created_at: String,
}

pub fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS preferences (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            rule       TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        );
        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )
}

/// Internal helper for Rust-side reads (e.g. transcript save settings).
pub fn get_setting_value(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .ok()
}

#[tauri::command]
pub fn get_setting(key: String, state: tauri::State<'_, DbState>) -> Result<Option<String>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(get_setting_value(&conn, &key))
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Setting {
    pub key: String,
    pub value: String,
}

#[tauri::command]
pub fn get_all_settings(state: tauri::State<'_, DbState>) -> Result<Vec<Setting>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT key, value FROM settings ORDER BY key")
        .map_err(|e| e.to_string())?;
    let settings = stmt
        .query_map([], |row| Ok(Setting { key: row.get(0)?, value: row.get(1)? }))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(settings)
}

#[tauri::command]
pub fn delete_setting(key: String, state: tauri::State<'_, DbState>) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM settings WHERE key = ?1", params![key])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn set_setting(
    key: String,
    value: String,
    state: tauri::State<'_, DbState>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn store_preference(
    rule: String,
    state: tauri::State<'_, DbState>,
) -> Result<Preference, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute("INSERT INTO preferences (rule) VALUES (?1)", params![rule])
        .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    let pref = conn
        .query_row(
            "SELECT id, rule, created_at FROM preferences WHERE id = ?1",
            params![id],
            |row| Ok(Preference { id: row.get(0)?, rule: row.get(1)?, created_at: row.get(2)? }),
        )
        .map_err(|e| e.to_string())?;
    Ok(pref)
}

#[tauri::command]
pub fn get_all_preferences(state: tauri::State<'_, DbState>) -> Result<Vec<Preference>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, rule, created_at FROM preferences ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let prefs = stmt
        .query_map([], |row| {
            Ok(Preference { id: row.get(0)?, rule: row.get(1)?, created_at: row.get(2)? })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(prefs)
}

#[tauri::command]
pub fn delete_preference(
    id: i64,
    state: tauri::State<'_, DbState>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM preferences WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
