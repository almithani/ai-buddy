use tauri::{Emitter, Manager};

mod accessibility;
mod download;
mod llm;
mod memory;
mod speech_assets;
mod transcription;

use memory::DbState;
use llm::LlmState;
use transcription::{TranscriptionActive, TranscriptStore};
use accessibility::PrevApp;

#[derive(Clone, serde::Serialize)]
pub struct PendingCapture {
    pub text: String,
    pub debug: String,
}

/// Capture from the last hotkey press — text + diagnostic log.
pub struct PendingText(pub std::sync::Mutex<PendingCapture>);


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
    show_chat_impl(&app)?;

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
        w.set_position(tauri::LogicalPosition::new(8.0, 8.0)).ok();
        w.show().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn show_chat_impl(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("chat") {
        // Only reposition when chat is not already visible; otherwise just focus it.
        if !w.is_visible().unwrap_or(false) {
            if let Some(droid) = app.get_webview_window("droid") {
                if let Ok(pos) = droid.outer_position() {
                    if let Ok(Some(monitor)) = droid.current_monitor() {
                        let scale = monitor.scale_factor();
                        let screen_h_logical = monitor.size().height as f64 / scale;
                        let droid_log_x = pos.x as f64 / scale;
                        let droid_log_y = pos.y as f64 / scale;
                        // 108 logical px to the right (100 droid width + 8 gap), tops aligned
                        let chat_x = droid_log_x + 108.0;
                        let chat_y = droid_log_y.min(screen_h_logical - 520.0);
                        w.set_position(tauri::LogicalPosition::new(chat_x, chat_y)).ok();
                    }
                }
            }
        }
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
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
fn reveal_in_finder(path: String) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    }
}

#[tauri::command]
fn open_speech_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_SpeechRecognition")
            .spawn();
    }
}

#[tauri::command]
fn show_chat(app: tauri::AppHandle) -> Result<(), String> {
    show_chat_impl(&app)
}

#[tauri::command]
fn hide_chat(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("chat") {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Called by ChatPanel after "hotkey-triggered". Returns the captured text
/// plus a diagnostic log, then clears state.
#[tauri::command]
fn get_pending_text(state: tauri::State<'_, PendingText>) -> PendingCapture {
    let mut guard = state.0.lock().unwrap();
    let capture = guard.clone();
    guard.text.clear();
    guard.debug.clear();
    capture
}

#[tauri::command]
fn read_file(path: String) -> Result<String, String> {
    let p = std::path::Path::new(&path);
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let name = p.file_name().unwrap_or_default().to_string_lossy();

    let image_exts = ["png","jpg","jpeg","gif","webp","bmp","svg","ico","tiff","heic"];
    if image_exts.contains(&ext.as_str()) {
        return Ok(format!("[Image file '{name}' — image content cannot be read as text]"));
    }
    if ext == "pdf" {
        return Ok(format!("[PDF file '{name}' — PDF text extraction is not supported]"));
    }

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.len() > 2_500 {
        Ok(format!("{}\n… (truncated — file too large for context window)", &content[..2_500]))
    } else {
        Ok(content)
    }
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

            // --- LLM state ---
            app.manage(LlmState(std::sync::Mutex::new(None)));

            // --- Transcription state ---
            eprintln!("[AiBuddy] backend up — transcript auto-save pipeline active");
            app.manage(TranscriptionActive(std::sync::atomic::AtomicBool::new(false)));
            app.manage(TranscriptStore {
                segments: std::sync::Mutex::new(Vec::new()),
                session_start: std::sync::Mutex::new(None),
                live_path: std::sync::Mutex::new(None),
                last_saved: std::sync::Mutex::new(None),
            });

            // --- Accessibility: previous frontmost app PID ---
            app.manage(PrevApp(std::sync::Mutex::new(None)));

            // --- Pending capture: filled by hotkey, read by chat window ---
            app.manage(PendingText(std::sync::Mutex::new(PendingCapture {
                text: String::new(),
                debug: String::new(),
            })));

            // --- Droid moved: keep chat tethered 108 logical px to the right ---
            if let Some(droid_w) = app.get_webview_window("droid") {
                let app_droid = app.handle().clone();
                droid_w.on_window_event(move |event| {
                    if let tauri::WindowEvent::Moved(new_phys) = event {
                        if let Some(chat) = app_droid.get_webview_window("chat") {
                            if chat.is_visible().unwrap_or(false) {
                                if let Some(droid) = app_droid.get_webview_window("droid") {
                                    if let Ok(Some(monitor)) = droid.current_monitor() {
                                        let scale = monitor.scale_factor();
                                        chat.set_position(tauri::LogicalPosition::new(
                                            new_phys.x as f64 / scale + 108.0,
                                            new_phys.y as f64 / scale,
                                        )).ok();
                                    }
                                }
                            }
                        }
                    }
                });
            }

            // --- Global hotkey: ⌥ Space ---
            {
                use tauri_plugin_global_shortcut::ShortcutState;
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_shortcut("Alt+Space")?
                        .with_handler(|app, _shortcut, event| {
                            if event.state() == ShortcutState::Pressed {
                                let prev = app.state::<PrevApp>();
                                let pending = app.state::<PendingText>();

                                // 1. Save frontmost PID (via NSWorkspace, no AX needed)
                                accessibility::save_prev_app_pid(&prev);
                                // 2. Capture selected text + diagnostic log
                                let (text, debug) = accessibility::capture_selected_text_debug(&prev);
                                // 3. Store so ChatPanel can read reliably
                                *pending.0.lock().unwrap() = PendingCapture { text, debug };
                                // 4. Show chat window
                                show_chat_impl(app).ok();
                                // 5. Send signal — no payload; chat calls get_pending_text()
                                if let Some(chat) = app.get_webview_window("chat") {
                                    let _ = chat.emit("hotkey-triggered", ());
                                }
                            }
                        })
                        .build(),
                )?;
            }

            // --- Window routing ---
            let is_onboarded = data_dir.join("onboarding_complete").exists();
            if is_onboarded {
                show_droid_window(app.handle())?;
                show_chat_impl(app.handle())?;
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
            get_pending_text,
            // Download
            download::check_model_exists,
            download::start_model_download,
            download::cancel_model_download,
            // LLM
            llm::load_model,
            llm::is_model_loaded,
            llm::generate_response,
            // Transcription
            transcription::transcription_auth_status,
            transcription::request_transcription_permission,
            speech_assets::speech_assets_status,
            speech_assets::install_speech_assets,
            open_speech_settings,
            transcription::is_transcribing,
            transcription::start_transcription,
            transcription::stop_transcription,
            transcription::get_transcript,
            transcription::get_transcript_files,
            reveal_in_finder,
            // Memory
            memory::store_preference,
            memory::get_setting,
            memory::set_setting,
            memory::get_memory,
            memory::delete_memory,
            // Accessibility
            accessibility::check_accessibility_permission,
            accessibility::replace_selected_text,
            // Files
            read_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
