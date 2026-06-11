use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;
use tauri::{AppHandle, Emitter, Manager};

#[cfg(target_os = "macos")]
use std::ffi::{c_char, c_void, CStr};

/// Managed state: true while a transcription session is running.
pub struct TranscriptionActive(pub AtomicBool);

/// Managed state: backend source of truth for the current/last transcript.
/// Survives frontend unmounts; cleared when a new session starts.
pub struct TranscriptStore {
    pub segments: Mutex<Vec<StoredSegment>>,
    pub session_start: Mutex<Option<SystemTime>>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSegment {
    pub source: String, // "me" | "them"
    pub text: String,
    pub ts_ms: u64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionSegment {
    pub source: String, // "me" | "them"
    pub text: String,
    pub is_final: bool,
}

static APP: OnceLock<AppHandle> = OnceLock::new();

// ── ObjC extern declarations (macOS only) ────────────────────────────────────

#[cfg(target_os = "macos")]
extern "C" {
    fn aibuddy_speech_auth_status() -> i32;
    fn aibuddy_speech_request_auth(
        cb: extern "C" fn(i32, *mut c_void),
        ctx: *mut c_void,
    );
    /// 0 = started; -1 = macOS < 13; -2 = unauthorized; -3 = on-device unavailable
    fn aibuddy_speech_start(
        cb: extern "C" fn(i32, *const c_char, bool, *mut c_void),
        ctx: *mut c_void,
    ) -> i32;
    fn aibuddy_speech_stop();
}

/// Fired by the ObjC speech lanes on arbitrary dispatch queues.
#[cfg(target_os = "macos")]
extern "C" fn on_speech(source: i32, text: *const c_char, is_final: bool, _ctx: *mut c_void) {
    let Some(app) = APP.get() else { return };
    if text.is_null() {
        return;
    }
    // The C string is only valid for the duration of this call — copy it now.
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned();

    if source < 0 {
        // Session-level error from ObjC: flip state off and notify the frontend.
        app.state::<TranscriptionActive>()
            .0
            .store(false, Ordering::SeqCst);
        app.emit("transcription-error", text).ok();
        return;
    }

    let source = if source == 0 { "me" } else { "them" }.to_string();

    if is_final {
        let ts_ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if let Ok(mut segs) = app.state::<TranscriptStore>().segments.lock() {
            segs.push(StoredSegment {
                source: source.clone(),
                text: text.clone(),
                ts_ms,
            });
            eprintln!("[AiBuddy] transcript store: {} segment(s)", segs.len());
        }
    }

    app.emit(
        "transcription-segment",
        TranscriptionSegment { source, text, is_final },
    )
    .ok();
}

fn auth_status_name(status: i32) -> &'static str {
    match status {
        3 => "authorized",
        1 => "denied",
        2 => "restricted",
        _ => "notDetermined",
    }
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn is_transcribing(state: tauri::State<'_, TranscriptionActive>) -> bool {
    state.0.load(Ordering::SeqCst)
}

#[tauri::command]
pub fn transcription_auth_status() -> String {
    #[cfg(target_os = "macos")]
    {
        auth_status_name(unsafe { aibuddy_speech_auth_status() }).to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        "restricted".to_string()
    }
}

#[cfg(target_os = "macos")]
extern "C" fn on_auth_result(status: i32, ctx: *mut c_void) {
    let tx = unsafe { Box::from_raw(ctx as *mut std::sync::mpsc::Sender<i32>) };
    tx.send(status).ok();
}

#[tauri::command]
pub async fn request_transcription_permission() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::channel::<i32>();
        let ctx = Box::into_raw(Box::new(tx)) as *mut c_void;
        unsafe { aibuddy_speech_request_auth(on_auth_result, ctx) };

        let status = tauri::async_runtime::spawn_blocking(move || {
            rx.recv_timeout(std::time::Duration::from_secs(120))
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|_| "Timed out waiting for permission response".to_string())?;

        Ok(auth_status_name(status).to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Speech recognition is only supported on macOS".to_string())
    }
}

#[tauri::command]
pub fn start_transcription(
    app: AppHandle,
    state: tauri::State<'_, TranscriptionActive>,
) -> Result<(), String> {
    if state.0.load(Ordering::SeqCst) {
        return Err("Transcription is already running — stop it first".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        APP.get_or_init(|| app.clone());
        let rc = unsafe { aibuddy_speech_start(on_speech, std::ptr::null_mut()) };
        match rc {
            0 => {
                let store = app.state::<TranscriptStore>();
                if let Ok(mut segs) = store.segments.lock() {
                    segs.clear();
                }
                if let Ok(mut start) = store.session_start.lock() {
                    *start = Some(SystemTime::now());
                }
                state.0.store(true, Ordering::SeqCst);
                app.emit("transcription-started", ()).ok();
                Ok(())
            }
            -1 => Err("Transcription requires macOS 13.0 or later".to_string()),
            -2 => Err(
                "Speech Recognition permission not granted — enable it in \
                 System Settings → Privacy & Security → Speech Recognition"
                    .to_string(),
            ),
            -3 => Err(
                "On-device speech recognition is unavailable for your language — \
                 enable Dictation in System Settings → Keyboard"
                    .to_string(),
            ),
            other => Err(format!("Failed to start transcription (code {other})")),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("Transcription is only supported on macOS".to_string())
    }
}

#[tauri::command]
pub fn stop_transcription(
    app: AppHandle,
    state: tauri::State<'_, TranscriptionActive>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    unsafe {
        aibuddy_speech_stop();
    }
    state.0.store(false, Ordering::SeqCst);
    app.emit("transcription-stopped", ()).ok();

    // The session's last final flushes up to ~3 s after stop (lanes stay alive
    // that long to receive it) — wait before saving so the file has the tail.
    eprintln!("[AiBuddy] transcription stopped — save scheduled in 3.5 s");
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(3500));
        save_transcript(&app);
    });
    Ok(())
}

#[tauri::command]
pub fn get_transcript(
    store: tauri::State<'_, TranscriptStore>,
) -> Result<Vec<StoredSegment>, String> {
    Ok(store.segments.lock().map_err(|e| e.to_string())?.clone())
}

// ── Saving to markdown ────────────────────────────────────────────────────────

/// Fold consecutive same-source segments into speaker turns.
fn fold_turns(segments: &[StoredSegment]) -> Vec<(String, u64, String)> {
    let mut turns: Vec<(String, u64, String)> = Vec::new();
    for seg in segments {
        match turns.last_mut() {
            Some((source, _, text)) if *source == seg.source => {
                text.push(' ');
                text.push_str(&seg.text);
            }
            _ => turns.push((seg.source.clone(), seg.ts_ms, seg.text.clone())),
        }
    }
    turns
}

fn sanitize_subject(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|c| !matches!(c, '/' | ':' | '\\' | '?' | '%' | '*' | '|' | '"' | '<' | '>' | '\n' | '\r'))
        .collect();
    let cleaned = cleaned.trim().to_string();
    let capped = if cleaned.len() > 40 {
        let cut = (0..=40).rev().find(|&i| cleaned.is_char_boundary(i)).unwrap_or(0);
        cleaned[..cut].trim_end().to_string()
    } else {
        cleaned
    };
    if capped.is_empty() { "Transcript".to_string() } else { capped }
}

fn fallback_subject(segments: &[StoredSegment]) -> String {
    let first = segments.iter().map(|s| s.text.as_str()).next().unwrap_or("");
    let words: Vec<&str> = first.split_whitespace().take(5).collect();
    sanitize_subject(&words.join(" "))
}

fn generate_subject(app: &AppHandle, segments: &[StoredSegment]) -> String {
    let transcript: String = {
        let mut t = String::new();
        for seg in segments {
            t.push_str(&seg.text);
            t.push(' ');
            if t.len() > 2000 {
                break;
            }
        }
        t
    };
    let prompt = format!(
        "Give a 3-5 word title for this meeting transcript. \
         Reply with ONLY the title — no punctuation, no quotes.\n\nTranscript:\n{transcript}"
    );
    let llm = app.state::<crate::llm::LlmState>();
    match crate::llm::generate_short_text(&llm, &prompt, 16) {
        Ok(s) if !s.trim().is_empty() => sanitize_subject(&s),
        _ => fallback_subject(segments),
    }
}

fn expand_home(raw: &str) -> std::path::PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(raw)
}

const DEFAULT_TRANSCRIPT_DIR: &str = "~/Documents/AI Buddy Transcripts";

/// Configured save dir, or the default when no setting exists (e.g. the user
/// deleted it in the Memory window).
fn transcript_save_dir(app: &AppHandle) -> std::path::PathBuf {
    let db = app.state::<crate::memory::DbState>();
    let configured = db
        .0
        .lock()
        .ok()
        .and_then(|conn| crate::memory::get_setting_value(&conn, "transcript_dir"));
    expand_home(&configured.unwrap_or_else(|| DEFAULT_TRANSCRIPT_DIR.to_string()))
}

/// Create dir, pick a non-colliding filename, write. Returns the final path.
fn write_transcript(
    dir: &std::path::Path,
    stem: &str,
    content: &str,
) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;

    // Avoid clobbering an existing file: " (2)", " (3)", …
    let mut path = dir.join(format!("{stem}.md"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem} ({n}).md"));
        n += 1;
    }

    std::fs::write(&path, content).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
    Ok(path)
}

/// On failure the transcript store is left untouched — the transcript stays
/// visible in the UI so the user can copy it manually.
fn save_transcript(app: &AppHandle) {
    let store = app.state::<TranscriptStore>();
    let segments: Vec<StoredSegment> = match store.segments.lock() {
        Ok(s) => s.clone(),
        Err(_) => return,
    };
    if segments.is_empty() {
        eprintln!("[AiBuddy] transcript save: store empty — nothing to write");
        return;
    }
    eprintln!("[AiBuddy] transcript save: {} segment(s), generating subject…", segments.len());
    let session_start = store
        .session_start
        .lock()
        .ok()
        .and_then(|g| *g)
        .unwrap_or_else(SystemTime::now);

    let subject = generate_subject(app, &segments);
    eprintln!("[AiBuddy] transcript save: subject = {subject:?}");

    let include_time = {
        let db = app.state::<crate::memory::DbState>();
        db.0.lock()
            .ok()
            .and_then(|conn| crate::memory::get_setting_value(&conn, "transcript_include_time"))
            .map(|v| v != "false")
            .unwrap_or(true)
    };

    let started: chrono::DateTime<chrono::Local> = session_start.into();
    let stem = if include_time {
        format!("{} - {}", started.format("%Y-%m-%d %H%M"), subject)
    } else {
        format!("{} - {}", started.format("%Y-%m-%d"), subject)
    };

    let mut md = format!(
        "# {}\n\n_Transcribed by AI Buddy on {}_\n\n",
        subject,
        started.format("%Y-%m-%d %-I:%M %p")
    );
    for (source, ts_ms, text) in fold_turns(&segments) {
        let label = if source == "me" { "Me" } else { "Them" };
        let when: chrono::DateTime<chrono::Local> =
            (SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(ts_ms)).into();
        md.push_str(&format!("**{}** ({}): {}\n\n", label, when.format("%-I:%M %p"), text));
    }

    let dir = transcript_save_dir(app);
    eprintln!("[AiBuddy] transcript save: writing to {}", dir.display());

    let result = write_transcript(&dir, &stem, &md).or_else(|first_err| {
        // Configured dir failed — fall back to the default location.
        let default_dir = expand_home(DEFAULT_TRANSCRIPT_DIR);
        if default_dir != dir {
            eprintln!("[AiBuddy] transcript save: {first_err} — falling back to {}", default_dir.display());
            write_transcript(&default_dir, &stem, &md)
                .map_err(|e2| format!("{first_err}; fallback also failed: {e2}"))
        } else {
            Err(first_err)
        }
    });

    match result {
        Ok(path) => {
            eprintln!("[AiBuddy] transcript saved: {}", path.display());
            app.emit("transcript-saved", path.to_string_lossy().to_string()).ok();
        }
        Err(e) => {
            eprintln!("[AiBuddy] transcript save FAILED: {e}");
            app.emit("transcript-save-failed", e).ok();
        }
    }
}
