use tauri::Manager;

mod accessibility;
mod download;
mod llm;
mod memory;

use memory::DbState;
use llm::LlmState;

#[tauri::command]
fn check_onboarding_complete(app: tauri::AppHandle) -> bool {
    let Ok(data_dir) = app.path().app_data_dir() else { return false };
    data_dir.join("onboarding_complete").exists()
}

#[tauri::command]
fn complete_onboarding(app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    std::fs::write(data_dir.join("onboarding_complete"), b"").map_err(|e| e.to_string())?;

    if let Some(w) = app.get_webview_window("onboarding") {
        w.close().map_err(|e| e.to_string())?;
    }

    show_droid_window(&app)?;

    // Kick off model load in the background now that onboarding is done
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app2.state::<LlmState>();
        if let Err(e) = llm::load_model(app2.clone(), state) {
            eprintln!("Model load error: {e}");
        }
    });

    Ok(())
}

fn show_droid_window(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("droid") {
        if let Ok(Some(monitor)) = w.current_monitor() {
            let size = monitor.size();
            let scale = monitor.scale_factor();
            let x = ((size.width as f64 / scale) - 120.0) * scale;
            let y = ((size.height as f64 / scale) - 120.0) * scale;
            w.set_position(tauri::PhysicalPosition::new(x as i32, y as i32)).ok();
        }
        w.show().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn get_platform() -> &'static str {
    if cfg!(target_os = "macos") { "macos" }
    else if cfg!(target_os = "windows") { "windows" }
    else { "linux" }
}

#[tauri::command]
fn request_accessibility_permission() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

#[tauri::command]
fn show_chat(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("chat") {
        if let Some(droid) = app.get_webview_window("droid") {
            if let Ok(pos) = droid.outer_position() {
                if let Ok(Some(monitor)) = droid.current_monitor() {
                    let scale = monitor.scale_factor();
                    let screen_h = monitor.size().height as f64;
                    let chat_w = 360.0 * scale;
                    let chat_h = 520.0 * scale;
                    let x = (pos.x as f64 - chat_w + 100.0 * scale).max(0.0);
                    let y = if pos.y as f64 - chat_h > 0.0 {
                        pos.y as f64 - chat_h - 8.0 * scale
                    } else {
                        (screen_h - chat_h - 120.0 * scale).max(0.0)
                    };
                    w.set_position(tauri::PhysicalPosition::new(x as i32, y as i32)).ok();
                }
            }
        }
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn hide_chat(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("chat") {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // --- SQLite ---
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let conn = rusqlite::Connection::open(data_dir.join("aibuddy.db"))?;
            memory::init_db(&conn)?;
            app.manage(DbState(std::sync::Mutex::new(conn)));

            // --- LLM state (model loaded lazily after onboarding) ---
            app.manage(LlmState(std::sync::Mutex::new(None)));

            // --- Window routing ---
            let is_onboarded = data_dir.join("onboarding_complete").exists();
            if is_onboarded {
                show_droid_window(app.handle())?;
                // Only load model if the file actually exists (may not if download was
                // interrupted or the app data dir was partially wiped).
                let app2 = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    match download::model_path(&app2) {
                        Ok(p) if p.exists() => {
                            let state = app2.state::<LlmState>();
                            if let Err(e) = llm::load_model(app2.clone(), state) {
                                eprintln!("Model load error: {e}");
                            }
                        }
                        _ => eprintln!("Model file not found — download it via the chat panel."),
                    }
                });
            } else {
                if let Some(w) = app.get_webview_window("onboarding") {
                    w.show()?;
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Onboarding / window management
            check_onboarding_complete,
            complete_onboarding,
            get_platform,
            request_accessibility_permission,
            show_chat,
            hide_chat,
            // Download
            download::check_model_exists,
            download::start_model_download,
            download::cancel_model_download,
            // LLM
            llm::load_model,
            llm::is_model_loaded,
            llm::generate_response,
            // Memory
            memory::store_preference,
            memory::get_all_preferences,
            memory::delete_preference,
            // Accessibility
            accessibility::check_accessibility_permission,
            accessibility::get_focused_text,
            accessibility::set_focused_text,
            accessibility::get_selected_text,
            accessibility::replace_selected_text,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
