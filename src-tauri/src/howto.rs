use serde::Deserialize;
use std::sync::OnceLock;

/// Shared with the frontend (`src/lib/howtoGuide.ts`). The LLM only supplies a
/// topic id; the app to open is the catalog's bundle id, never model output.
const CATALOG_JSON: &str = include_str!("../../src/lib/howtoCatalog.json");

#[derive(Deserialize)]
struct Howto {
    id: String,
    app: Option<String>,
}

fn catalog() -> &'static [Howto] {
    static CATALOG: OnceLock<Vec<Howto>> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(CATALOG_JSON).expect("howtoCatalog.json is invalid"))
}

/// Opens the app a how-to topic is about (e.g. Mail for "attach a photo").
/// Ok(false) when the topic has no app.
#[tauri::command]
pub fn open_howto_app(topic: String) -> Result<bool, String> {
    let t = catalog()
        .iter()
        .find(|t| t.id == topic)
        .ok_or_else(|| format!("Unknown how-to topic: {topic}"))?;
    let Some(bundle_id) = &t.app else { return Ok(false) };
    #[cfg(target_os = "macos")]
    {
        // `open -b` fails for apps that aren't installed (e.g. Zoom).
        let status = std::process::Command::new("open")
            .args(["-b", bundle_id])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("The app for this isn't installed ({bundle_id})"));
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = bundle_id;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_with_unique_ids() {
        let mut ids: Vec<&str> = catalog().iter().map(|t| t.id.as_str()).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(n, ids.len(), "duplicate how-to ids");
        assert!(n > 0);
    }
}
