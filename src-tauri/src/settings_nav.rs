use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tauri::AppHandle;

/// Shared with the frontend (`src/lib/settingsGuide.ts`) so a topic id means the
/// same page on both sides. The LLM only ever supplies a topic id; the URL is
/// built from this compiled-in catalog, never from model output.
const CATALOG_JSON: &str = include_str!("../../src/lib/settingsCatalog.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Topic {
    id: String,
    pane: String,
    anchor: Option<String>,
    control_id: Option<String>,
    control_label: Option<String>,
}

fn catalog() -> &'static [Topic] {
    static CATALOG: OnceLock<Vec<Topic>> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(CATALOG_JSON).expect("settingsCatalog.json is invalid"))
}

#[derive(Serialize)]
pub struct OpenResult {
    /// The control was found on screen and is ringed.
    found: bool,
    /// Plain-language current state ("switched off", "slider at about 30% ..."),
    /// empty when unknown.
    state: String,
}

#[tauri::command]
pub async fn open_system_settings(app: AppHandle, topic: String) -> Result<OpenResult, String> {
    let t = catalog()
        .iter()
        .find(|t| t.id == topic)
        .ok_or_else(|| format!("Unknown settings topic: {topic}"))?;
    let url = match &t.anchor {
        Some(anchor) => format!("x-apple.systempreferences:{}?{}", t.pane, anchor),
        None => format!("x-apple.systempreferences:{}", t.pane),
    };
    #[cfg(target_os = "macos")]
    {
        let control_id = t.control_id.clone().unwrap_or_default();
        let control_label = t.control_label.clone().unwrap_or_default();
        let app2 = app.clone();
        tauri::async_runtime::spawn_blocking(move || mac::open_and_ring(&app2, &url, &control_id, &control_label))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, url);
        Err("System Settings navigation is only supported on macOS".into())
    }
}

/// Quick facts for questions that aren't a single settings control ("is my Mac
/// up to date?", "why is there no sound?"). Fixed read-only commands only — no
/// model output reaches a command line.
#[tauri::command]
pub async fn get_mac_info() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        tauri::async_runtime::spawn_blocking(mac::mac_info)
            .await
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Only supported on macOS".into())
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};
    use tauri::Manager;

    extern "C" {
        fn aibuddy_settings_find_control(
            control_id: *const c_char,
            control_label: *const c_char,
            timeout_ms: i32,
            state_out: *mut c_char,
            state_len: i32,
            x: *mut f64,
            y: *mut f64,
            w: *mut f64,
            h: *mut f64,
        ) -> i32;
        fn aibuddy_settings_tracked_frame(x: *mut f64, y: *mut f64, w: *mut f64, h: *mut f64) -> i32;
        fn aibuddy_settings_stop_tracking();
        fn aibuddy_settings_is_running() -> i32;
        fn aibuddy_settings_ring_context_active() -> i32;
        fn aibuddy_settings_quit();
        fn aibuddy_order_front_passive(ns_window: *mut c_void);
    }

    const RING_PAD: f64 = 10.0;
    const RING_LIFETIME: Duration = Duration::from_secs(20);
    const RING_TICK: Duration = Duration::from_millis(250);

    /// Bumped on every open so an older ring ticker stops when a new topic opens.
    static RING_GENERATION: AtomicU64 = AtomicU64::new(0);

    type Rect = (f64, f64, f64, f64);

    fn find(control_id: &str, control_label: &str, timeout_ms: i32) -> Option<(String, Rect)> {
        let cid = CString::new(control_id).ok()?;
        let clabel = CString::new(control_label).ok()?;
        let mut state = [0 as c_char; 256];
        let (mut x, mut y, mut w, mut h) = (0.0, 0.0, 0.0, 0.0);
        let rc = unsafe {
            aibuddy_settings_find_control(
                cid.as_ptr(),
                clabel.as_ptr(),
                timeout_ms,
                state.as_mut_ptr(),
                state.len() as i32,
                &mut x,
                &mut y,
                &mut w,
                &mut h,
            )
        };
        if rc != 1 {
            eprintln!("[settings_nav] control '{control_id}{control_label}' not found (rc={rc})");
            return None;
        }
        let state = unsafe { CStr::from_ptr(state.as_ptr()) }.to_string_lossy().into_owned();
        Some((state, (x, y, w, h)))
    }

    fn run(cmd: &str, args: &[&str]) -> String {
        std::process::Command::new(cmd)
            .args(args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    }

    pub fn mac_info() -> String {
        let mut lines = Vec::new();

        let version = run("sw_vers", &["-productVersion"]);
        if !version.is_empty() {
            lines.push(format!("macOS version: {version}"));
        }

        // "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=…)\t85%; discharging; 4:12 remaining …"
        let batt = run("pmset", &["-g", "batt"]);
        let source = batt.split('\'').nth(1).unwrap_or("");
        if let Some(detail) = batt.lines().nth(1).and_then(|l| l.split('\t').nth(1)) {
            let detail = detail.split(" present").next().unwrap_or(detail).trim();
            lines.push(format!("Battery: {detail} (on {source})"));
        } else if !source.is_empty() {
            lines.push(format!("Power: {source} (no battery)"));
        }

        // Wi-Fi hardware device name (usually en0), then its power state.
        let ports = run("networksetup", &["-listallhardwareports"]);
        let wifi_dev = ports
            .split("\n\n")
            .find(|block| block.contains("Wi-Fi"))
            .and_then(|block| block.lines().find_map(|l| l.strip_prefix("Device: ")))
            .map(str::to_string);
        if let Some(dev) = wifi_dev {
            let power = run("networksetup", &["-getairportpower", &dev]);
            if let Some(state) = power.rsplit(": ").next().filter(|s| !s.is_empty()) {
                lines.push(format!("Wi-Fi: {state}"));
            }
        }

        let volume = run("osascript", &["-e", "output volume of (get volume settings)"]);
        let muted = run("osascript", &["-e", "output muted of (get volume settings)"]);
        if !volume.is_empty() && volume != "missing value" {
            let mute_note = if muted == "true" { " — MUTED" } else { "" };
            lines.push(format!("Sound volume: {volume}%{mute_note}"));
        }

        if lines.is_empty() {
            "Couldn't read Mac info.".into()
        } else {
            lines.join("\n")
        }
    }

    fn open_url(url: &str) -> Result<(), String> {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn open_and_ring(app: &AppHandle, url: &str, control_id: &str, control_label: &str) -> Result<OpenResult, String> {
        let generation = RING_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        hide_ring(app);
        unsafe { aibuddy_settings_stop_tracking() };

        let was_running = unsafe { aibuddy_settings_is_running() } == 1;
        let has_control = !control_id.is_empty() || !control_label.is_empty();

        // Deep links don't navigate away from some sub-pages (e.g. Text Size), so
        // an already-open System Settings may ignore the link. A fresh launch
        // always lands; with no control to verify against, relaunch up front.
        if was_running && !has_control {
            unsafe { aibuddy_settings_quit() };
        }
        open_url(url)?;
        if !has_control {
            return Ok(OpenResult { found: false, state: String::new() });
        }

        let mut hit = find(control_id, control_label, 4000);
        if hit.is_none() && was_running {
            unsafe { aibuddy_settings_quit() };
            open_url(url)?;
            hit = find(control_id, control_label, 5000);
        }
        let Some((state, rect)) = hit else {
            return Ok(OpenResult { found: false, state: String::new() });
        };

        show_ring(app, rect);
        let app2 = app.clone();
        std::thread::spawn(move || track_ring(app2, generation));
        Ok(OpenResult { found: true, state })
    }

    fn show_ring(app: &AppHandle, (x, y, w, h): Rect) {
        let Some(win) = app.get_webview_window("highlight") else { return };
        let _ = win.set_focusable(false);
        let _ = win.set_ignore_cursor_events(true);
        let _ = win.set_position(tauri::LogicalPosition::new(x - RING_PAD, y - RING_PAD));
        let _ = win.set_size(tauri::LogicalSize::new(w + 2.0 * RING_PAD, h + 2.0 * RING_PAD));
        let win2 = win.clone();
        let _ = app.run_on_main_thread(move || {
            if let Ok(ns) = win2.ns_window() {
                unsafe { aibuddy_order_front_passive(ns) };
            }
        });
    }

    fn hide_ring(app: &AppHandle) {
        if let Some(win) = app.get_webview_window("highlight") {
            let _ = win.hide();
        }
    }

    /// Keeps the ring on the control while it scrolls/moves; hides it after
    /// RING_LIFETIME, when the control disappears (page changed), when the user
    /// switches to another app, or when a newer topic is opened.
    fn track_ring(app: AppHandle, generation: u64) {
        let started = Instant::now();
        let mut last: Option<Rect> = None;
        loop {
            std::thread::sleep(RING_TICK);
            if RING_GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            let active = unsafe { aibuddy_settings_ring_context_active() } == 1;
            let (mut x, mut y, mut w, mut h) = (0.0, 0.0, 0.0, 0.0);
            let visible = unsafe { aibuddy_settings_tracked_frame(&mut x, &mut y, &mut w, &mut h) } == 1;
            if !active || !visible || started.elapsed() > RING_LIFETIME {
                break;
            }
            if last != Some((x, y, w, h)) {
                if let Some(win) = app.get_webview_window("highlight") {
                    let _ = win.set_position(tauri::LogicalPosition::new(x - RING_PAD, y - RING_PAD));
                    let _ = win.set_size(tauri::LogicalSize::new(w + 2.0 * RING_PAD, h + 2.0 * RING_PAD));
                }
                last = Some((x, y, w, h));
            }
        }
        if RING_GENERATION.load(Ordering::SeqCst) == generation {
            hide_ring(&app);
            unsafe { aibuddy_settings_stop_tracking() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_with_unique_ids() {
        let ids: Vec<&str> = catalog().iter().map(|t| t.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len(), "duplicate topic ids");
        assert!(!ids.is_empty());
    }
}
