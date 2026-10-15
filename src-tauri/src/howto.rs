use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Shared with the frontend (`src/lib/howtoGuide.ts`). The LLM only supplies a
/// topic id; the app or web address to open comes from the catalog, never from
/// model output.
const CATALOG_JSON: &str = include_str!("../../src/lib/howtoCatalog.json");

#[derive(Deserialize)]
struct Howto {
    id: String,
    app: Option<String>,
    /// Per-service versions, e.g. "gmail" for the email guides.
    #[serde(default)]
    variants: HashMap<String, Variant>,
}

#[derive(Deserialize)]
struct Variant {
    url: String,
}

fn catalog() -> &'static [Howto] {
    static CATALOG: OnceLock<Vec<Howto>> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(CATALOG_JSON).expect("howtoCatalog.json is invalid"))
}

/// Opens the app a how-to topic is about (e.g. Mail for "attach a photo"), or,
/// for a `variant` like "gmail", that variant's web page in the default
/// browser. Ok(false) when there's nothing to open.
#[tauri::command]
pub fn open_howto_app(topic: String, variant: Option<String>) -> Result<bool, String> {
    let t = catalog()
        .iter()
        .find(|t| t.id == topic)
        .ok_or_else(|| format!("Unknown how-to topic: {topic}"))?;
    let args: Vec<&str> = match variant.as_deref() {
        Some(v) => {
            let variant = t.variants.get(v).ok_or_else(|| format!("No {v} version of how-to {topic}"))?;
            vec![variant.url.as_str()]
        }
        None => match &t.app {
            Some(bundle_id) => vec!["-b", bundle_id.as_str()],
            None => return Ok(false),
        },
    };
    #[cfg(target_os = "macos")]
    {
        // `open -b` fails for apps that aren't installed (e.g. Zoom).
        let status = std::process::Command::new("open")
            .args(&args)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("Couldn't open {}", args.last().unwrap_or(&"it")));
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = args;
    Ok(true)
}

/// Bundle id of the app this Mac opens email links with (`mailto:`), e.g.
/// "com.apple.mail". Used to guess Mail vs. web email before the user has said
/// which they use.
#[tauri::command]
pub fn default_email_app() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        use core_foundation::base::TCFType;
        use core_foundation::string::{CFString, CFStringRef};
        #[link(name = "CoreServices", kind = "framework")]
        extern "C" {
            fn LSCopyDefaultHandlerForURLScheme(scheme: CFStringRef) -> CFStringRef;
        }
        let scheme = CFString::new("mailto");
        let handler = unsafe { LSCopyDefaultHandlerForURLScheme(scheme.as_concrete_TypeRef()) };
        if handler.is_null() {
            return None;
        }
        Some(unsafe { CFString::wrap_under_create_rule(handler) }.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    None
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

    #[test]
    fn email_guides_have_https_gmail_versions() {
        for id in ["email_new", "email_attach", "email_reply"] {
            let t = catalog().iter().find(|t| t.id == id).expect(id);
            let gmail = t.variants.get("gmail").expect("gmail variant");
            assert!(gmail.url.starts_with("https://mail.google.com/"), "{id}: {}", gmail.url);
        }
    }

    #[test]
    fn unknown_variant_is_refused() {
        assert!(open_howto_app("email_new".into(), Some("hotmail".into())).is_err());
    }
}
