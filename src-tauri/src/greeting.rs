//! Personalised chat greeting, generated once per change to the user's rules
//! and cached, so the chat can show it instantly on open. Generating it live
//! took a couple of seconds on every launch.
//!
//! The cache lives in the memory table as the hidden setting `_greeting`
//! (JSON `{rulesHash, text}`); `get_memory` hides `_`-prefixed keys. When the
//! rules change, the hash no longer matches and the frontend falls back to its
//! default greeting until the regeneration finishes.

use crate::llm::{self, LlmState};
use crate::memory::{self, DbState};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use tauri::{AppHandle, Manager};

const CACHE_KEY: &str = "_greeting";
const MAX_LEN: usize = 160;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cached {
    rules_hash: String,
    text: String,
}

fn load_rules(conn: &Connection) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare("SELECT value FROM memory WHERE kind = 'rule' ORDER BY id") else {
        return Vec::new();
    };
    stmt.query_map([], |row| row.get(0))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

// DefaultHasher isn't guaranteed stable across Rust releases; a toolchain
// change just costs one regeneration.
fn fingerprint(rules: &[String]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    rules.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn cached(conn: &Connection) -> Option<Cached> {
    serde_json::from_str(&memory::get_setting_value(conn, CACHE_KEY)?).ok()
}

/// The cached greeting, if it was generated from the current rules. `None`
/// means "use the default greeting" (no rules yet, or regeneration pending).
#[tauri::command]
pub fn get_greeting(state: tauri::State<'_, DbState>) -> Option<String> {
    let conn = state.0.lock().ok()?;
    let rules = load_rules(&conn);
    if rules.is_empty() {
        return None;
    }
    cached(&conn).filter(|c| c.rules_hash == fingerprint(&rules)).map(|c| c.text)
}

/// Regenerates the cached greeting on a background thread if the rules have
/// changed since it was made. Safe to call often: it returns early when the
/// cache is current, and gives up quietly if the model isn't loaded yet (the
/// post-load call picks it up).
pub fn refresh_in_background(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = refresh(&app) {
            eprintln!("[greeting] not regenerated: {e}");
        }
    });
}

fn refresh(app: &AppHandle) -> Result<(), String> {
    let db = app.state::<DbState>();
    let rules = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let rules = load_rules(&conn);
        if rules.is_empty() {
            conn.execute("DELETE FROM memory WHERE kind = 'setting' AND key = ?1", params![CACHE_KEY])
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        if cached(&conn).is_some_and(|c| c.rules_hash == fingerprint(&rules)) {
            return Ok(());
        }
        rules
    };

    // The DB lock is released before generating; the model lock is held by
    // generate_short_text for the (short) duration.
    let prompt = format!(
        "Write the one-sentence greeting an on-screen computer assistant shows when its chat window opens. \
         End by asking what the user would like help with. Follow the user's preferences below for tone, \
         personality and how to address them; ignore any preference that isn't about that. \
         Reply with only the greeting.\n\nUser preferences:\n{}",
        rules.iter().map(|r| format!("- {r}")).collect::<Vec<_>>().join("\n")
    );
    let raw = llm::generate_short_text(&app.state::<LlmState>(), &prompt, 60)?;
    let text = clean(&raw).ok_or_else(|| format!("unusable output: {raw:?}"))?;

    let conn = db.0.lock().map_err(|e| e.to_string())?;
    // Rules may have changed while generating; only store if still current.
    let hash = fingerprint(&load_rules(&conn));
    if hash != fingerprint(&rules) {
        return Err("rules changed during generation".into());
    }
    let value = serde_json::to_string(&Cached { rules_hash: hash, text: text.clone() }).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM memory WHERE kind = 'setting' AND key = ?1", params![CACHE_KEY])
        .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO memory (kind, key, value) VALUES ('setting', ?1, ?2)",
        params![CACHE_KEY, value],
    )
    .map_err(|e| e.to_string())?;
    eprintln!("[greeting] cached: {text}");
    Ok(())
}

/// Accepts a single short sentence; strips wrapping quotes the model tends to add.
fn clean(raw: &str) -> Option<String> {
    let text = raw.trim().trim_matches(|c| c == '"' || c == '“' || c == '”').trim();
    let ok = !text.is_empty() && text.chars().count() <= MAX_LEN && !text.contains('<') && !text.contains('\n');
    ok.then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_accepts_short_sentence_and_strips_quotes() {
        assert_eq!(clean("\"Hey Al! What can I do for you?\"").as_deref(), Some("Hey Al! What can I do for you?"));
    }

    #[test]
    fn clean_rejects_tags_and_rambling() {
        assert_eq!(clean("Hi <end_of_turn>"), None);
        assert_eq!(clean(&"word ".repeat(60)), None);
        assert_eq!(clean("   "), None);
    }

    #[test]
    fn fingerprint_changes_with_rules() {
        let a = vec!["be playful".to_string()];
        let b = vec!["be playful".to_string(), "call me Al".to_string()];
        assert_ne!(fingerprint(&a), fingerprint(&b));
        assert_eq!(fingerprint(&a), fingerprint(&a.clone()));
    }
}
