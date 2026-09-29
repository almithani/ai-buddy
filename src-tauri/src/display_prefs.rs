//! The chat follows macOS display accessibility settings: text size, Increase
//! contrast, Reduce transparency, Reduce motion. Light/Dark is pure CSS
//! (`prefers-color-scheme`). Changes are pushed to every window as
//! `display-prefs-changed`; text-size changes also resize the chat window so
//! the zoomed UI keeps the same layout.

use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DisplayPrefs {
    /// Content-size category, e.g. "L" (default), "XL", "AX2".
    text_size: String,
    /// Zoom factor for the chat UI (1.0 = default size).
    scale: f64,
    increase_contrast: bool,
    reduce_transparency: bool,
    reduce_motion: bool,
}

const CHAT_BASE_SIZE: (f64, f64) = (360.0, 520.0);
const MAX_SCALE: f64 = 2.0;

/// iOS/macOS body text size per category (pt) relative to the default L (17 pt).
fn scale_for(category: &str) -> f64 {
    let body_pt = match category {
        "XS" => 14.0,
        "S" => 15.0,
        "M" => 16.0,
        "L" => 17.0,
        "XL" => 19.0,
        "XXL" => 21.0,
        "XXXL" => 23.0,
        "AccessibilityM" | "AX1" => 28.0,
        "AccessibilityL" | "AX2" => 33.0,
        "AccessibilityXL" | "AX3" => 40.0,
        "AccessibilityXXL" | "AX4" => 47.0,
        "AccessibilityXXXL" | "AX5" => 53.0,
        _ => 17.0,
    };
    (body_pt / 17.0_f64).min(MAX_SCALE)
}

#[cfg(target_os = "macos")]
fn read() -> DisplayPrefs {
    use std::ffi::{c_char, CStr};
    extern "C" {
        fn aibuddy_display_prefs(category: *mut c_char, category_len: i32, contrast: *mut i32, transparency: *mut i32, motion: *mut i32);
    }
    let mut buf = [0 as c_char; 64];
    let (mut contrast, mut transparency, mut motion) = (0, 0, 0);
    unsafe { aibuddy_display_prefs(buf.as_mut_ptr(), buf.len() as i32, &mut contrast, &mut transparency, &mut motion) };
    let text_size = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned();
    DisplayPrefs {
        scale: scale_for(&text_size),
        text_size,
        increase_contrast: contrast != 0,
        reduce_transparency: transparency != 0,
        reduce_motion: motion != 0,
    }
}

#[cfg(not(target_os = "macos"))]
fn read() -> DisplayPrefs {
    DisplayPrefs { text_size: "L".into(), scale: 1.0, increase_contrast: false, reduce_transparency: false, reduce_motion: false }
}

#[tauri::command]
pub fn get_display_prefs() -> DisplayPrefs {
    read()
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static LAST: Mutex<Option<DisplayPrefs>> = Mutex::new(None);

/// Re-reads the settings and, if anything changed, notifies the windows.
fn check(app: &AppHandle) {
    let prefs = read();
    let previous = {
        let Ok(mut last) = LAST.lock() else { return };
        if last.as_ref() == Some(&prefs) {
            return;
        }
        last.replace(prefs.clone())
    };
    if previous.as_ref().map(|p| p.scale) != Some(prefs.scale) {
        resize_chat(app, prefs.scale);
    }
    let _ = app.emit("display-prefs-changed", prefs);
}

fn resize_chat(app: &AppHandle, scale: f64) {
    let Some(chat) = app.get_webview_window("chat") else { return };
    let (mut w, mut h) = (CHAT_BASE_SIZE.0 * scale, CHAT_BASE_SIZE.1 * scale);
    if let Ok(Some(monitor)) = chat.current_monitor() {
        let s = monitor.scale_factor();
        w = w.min(monitor.size().width as f64 / s - 40.0);
        h = h.min(monitor.size().height as f64 / s - 80.0);
    }
    let _ = chat.set_size(tauri::LogicalSize::new(w, h));
}

extern "C" fn on_display_options_changed() {
    if let Some(app) = APP.get() {
        check(app);
    }
}

/// Applies the current settings once, then watches for changes: an NSWorkspace
/// notification for contrast/transparency/motion, and a 1.5 s poll for text
/// size (which has no change notification).
pub fn start_watching(app: &AppHandle) {
    if APP.set(app.clone()).is_err() {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn aibuddy_display_prefs_observe(cb: extern "C" fn());
        }
        unsafe { aibuddy_display_prefs_observe(on_display_options_changed) };
    }
    let app = app.clone();
    std::thread::spawn(move || loop {
        check(&app);
        std::thread::sleep(std::time::Duration::from_millis(1500));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_size_is_unscaled() {
        assert_eq!(scale_for("L"), 1.0);
        assert_eq!(scale_for("something-unknown"), 1.0);
    }

    #[test]
    fn larger_categories_scale_up_and_cap() {
        assert!(scale_for("XL") > 1.0);
        assert!(scale_for("XXXL") > scale_for("XL"));
        assert!(scale_for("XS") < 1.0);
        assert_eq!(scale_for("AccessibilityXXXL"), MAX_SCALE);
    }
}
