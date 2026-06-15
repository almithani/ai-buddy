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
    /// The in-progress meeting-notes file, created at session start and
    /// rewritten on every final segment; renamed to its real name on save.
    pub live_path: Mutex<Option<std::path::PathBuf>>,
    /// The most recent successfully saved transcript file.
    pub last_saved: Mutex<Option<std::path::PathBuf>>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSegment {
    pub source: String, // "me" | "them" | "Speaker N" (after diarization)
    pub text: String,
    pub ts_ms: u64,
    /// Audio-relative seconds (None for the legacy engine / unknown).
    pub start_sec: Option<f64>,
    pub end_sec: Option<f64>,
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
        cb: extern "C" fn(i32, *const c_char, bool, f64, f64, *mut c_void),
        ctx: *mut c_void,
        record_wav_path: *const c_char,
    ) -> i32;
    fn aibuddy_speech_stop();
}

/// Fired by the ObjC speech lanes on arbitrary dispatch queues.
#[cfg(target_os = "macos")]
extern "C" fn on_speech(
    source: i32,
    text: *const c_char,
    is_final: bool,
    start_sec: f64,
    end_sec: f64,
    _ctx: *mut c_void,
) {
    let Some(app) = APP.get() else { return };
    if text.is_null() {
        return;
    }
    // The C string is only valid for the duration of this call — copy it now.
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned();

    if source == -2 {
        // Non-fatal warning (e.g. a lane cooling down after repeated errors).
        app.emit("transcription-warning", text).ok();
        return;
    }
    if source < 0 {
        // Fatal session error: flip state off, notify the frontend, and save
        // whatever we have — an abnormal stop must not lose the transcript.
        app.state::<TranscriptionActive>()
            .0
            .store(false, Ordering::SeqCst);
        app.emit("transcription-error", text).ok();
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            save_transcript(&app);
        });
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
                start_sec: (start_sec >= 0.0).then_some(start_sec),
                end_sec: (end_sec >= 0.0).then_some(end_sec),
            });
            eprintln!("[AiBuddy] transcript store: {} segment(s)", segs.len());
        }
        update_live_file(app);
    }

    app.emit(
        "transcription-segment",
        TranscriptionSegment { source, text, is_final },
    )
    .ok();
}

/// Re-render the whole meeting-notes file from the store. Called on every
/// final segment — files are tiny, and a full rewrite keeps the live file in
/// exactly the format the final save produces.
fn update_live_file(app: &AppHandle) {
    let store = app.state::<TranscriptStore>();
    let Some(path) = store.live_path.lock().ok().and_then(|g| g.clone()) else {
        return;
    };
    let Ok(segments) = store.segments.lock().map(|s| s.clone()) else {
        return;
    };
    let started: chrono::DateTime<chrono::Local> = store
        .session_start
        .lock()
        .ok()
        .and_then(|g| *g)
        .unwrap_or_else(SystemTime::now)
        .into();
    let md = render_markdown("Meeting in progress", None, started, &segments);
    if let Err(e) = std::fs::write(&path, md) {
        eprintln!("[AiBuddy] live notes write failed ({}): {e}", path.display());
    }
}

/// Temp WAV holding the current session's Them stream (for diarization).
/// Deleted after the save attributes speakers.
fn them_session_wav_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("them-session.wav"))
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

        // Record the Them stream for post-meeting diarization, but only when
        // the diarization models are installed (otherwise no recording at all).
        let wav = them_session_wav_path(&app);
        let record_c: Option<std::ffi::CString> =
            if crate::diarization::models_installed(&app) {
                if let Some(ref p) = wav {
                    std::fs::remove_file(p).ok(); // clear any stale recording
                }
                wav.as_ref()
                    .and_then(|p| p.to_str())
                    .and_then(|s| std::ffi::CString::new(s).ok())
            } else {
                None
            };
        let record_ptr = record_c
            .as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(std::ptr::null());

        let rc = unsafe { aibuddy_speech_start(on_speech, std::ptr::null_mut(), record_ptr) };
        match rc {
            0 => {
                let now = SystemTime::now();
                let store = app.state::<TranscriptStore>();
                if let Ok(mut segs) = store.segments.lock() {
                    segs.clear();
                }
                if let Ok(mut start) = store.session_start.lock() {
                    *start = Some(now);
                }

                // Create the live meeting-notes file up front so the user can
                // watch it grow; renamed to its real subject on save.
                let live = create_live_file(&app, now);
                if let Ok(mut lp) = store.live_path.lock() {
                    *lp = live.clone();
                }

                state.0.store(true, Ordering::SeqCst);
                let live_str = live
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                app.emit("transcription-started", live_str).ok();
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

    // The session's last text flushes up to ~4 s after stop (lanes stay alive
    // past the 2 s pending-flush timeout) — wait before saving so the file
    // has the tail.
    eprintln!("[AiBuddy] transcription stopped — save scheduled in 4.5 s");
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(4500));
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

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptFiles {
    pub live: Option<String>,
    pub saved: Option<String>,
}

#[tauri::command]
pub fn get_transcript_files(store: tauri::State<'_, TranscriptStore>) -> TranscriptFiles {
    let path_str = |g: &Mutex<Option<std::path::PathBuf>>| {
        g.lock()
            .ok()
            .and_then(|p| p.as_ref().map(|p| p.to_string_lossy().to_string()))
    };
    TranscriptFiles {
        live: path_str(&store.live_path),
        saved: path_str(&store.last_saved),
    }
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

/// Bulleted meeting summary (Key Points / Decisions / Action Items). Returns ""
/// on error or empty input — the caller then omits the summary body.
fn generate_summary(app: &AppHandle, segments: &[StoredSegment]) -> String {
    // Speaker-labeled transcript so the model can attribute decisions/actions.
    let transcript: String = fold_turns(segments)
        .into_iter()
        .map(|(source, _ts, text)| {
            let label = match source.as_str() {
                "me" => "Me",
                "them" => "Them",
                other => other,
            };
            format!("{label}: {text}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    if transcript.trim().is_empty() {
        return String::new();
    }

    let prompt = format!(
        "You are summarizing a meeting transcript into concise minutes. \
         Write short markdown bullet points under exactly these three headings \
         (omit a heading only if it has nothing):\n\
         **Key Points**\n**Decisions**\n**Action Items**\n\n\
         Be specific and factual. Do not invent content.\n\nTranscript:\n{transcript}"
    );
    let llm = app.state::<crate::llm::LlmState>();
    crate::llm::generate_text(&llm, &prompt, 400).unwrap_or_default()
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

/// Shared renderer for the live file and the final save. `summary` is the
/// AI-generated meeting summary (None during the live session — shows a
/// placeholder; Some at save time).
fn render_markdown(
    subject: &str,
    summary: Option<&str>,
    started: chrono::DateTime<chrono::Local>,
    segments: &[StoredSegment],
) -> String {
    let mut md = format!(
        "# {}\n\n_Transcribed by AI Buddy on {}_\n\n",
        subject,
        started.format("%Y-%m-%d %-I:%M %p")
    );

    md.push_str("## AI-Generated Summary\n\n");
    match summary {
        Some(s) if !s.trim().is_empty() => {
            md.push_str(s.trim());
            md.push_str("\n\n");
        }
        _ => md.push_str("_Generated when the session ends._\n\n"),
    }

    md.push_str("## Transcript\n\n");
    for (source, ts_ms, text) in fold_turns(segments) {
        // After diarization, `source` is already a speaker label (e.g. "Speaker 1").
        let label = match source.as_str() {
            "me" => "Me".to_string(),
            "them" => "Them".to_string(),
            other => other.to_string(),
        };
        let when: chrono::DateTime<chrono::Local> =
            (SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(ts_ms)).into();
        md.push_str(&format!("**{}** ({}): {}\n\n", label, when.format("%-I:%M %p"), text));
    }
    md
}

/// Replace the "them" source of each segment with a "Speaker N" label by
/// intersecting its audio time range against diarization segments. Segments
/// without a time range, or with no overlap, keep "them". "me" is untouched.
fn apply_diarization(segments: &mut [StoredSegment], speakers: &[crate::diarization::SpeakerSegment]) {
    if speakers.is_empty() {
        return;
    }
    // Map raw speaker index → 1-based display number in first-appearance order.
    let mut order: Vec<i32> = Vec::new();
    let mut display = |raw: i32| -> usize {
        if let Some(pos) = order.iter().position(|&s| s == raw) {
            pos + 1
        } else {
            order.push(raw);
            order.len()
        }
    };

    for seg in segments.iter_mut() {
        if seg.source != "them" {
            continue;
        }
        let (Some(start), Some(end)) = (seg.start_sec, seg.end_sec) else { continue };
        // Pick the speaker whose segment overlaps this one the most.
        let mut best_raw: Option<i32> = None;
        let mut best_overlap = 0.0_f64;
        for sp in speakers {
            let ov = (end.min(sp.end as f64) - start.max(sp.start as f64)).max(0.0);
            if ov > best_overlap {
                best_overlap = ov;
                best_raw = Some(sp.speaker);
            }
        }
        if let Some(raw) = best_raw {
            seg.source = format!("Speaker {}", display(raw));
        }
    }
}

/// Pick a non-colliding `<stem>.md` path in `dir`.
fn unique_md_path(dir: &std::path::Path, stem: &str) -> std::path::PathBuf {
    let mut path = dir.join(format!("{stem}.md"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem} ({n}).md"));
        n += 1;
    }
    path
}

/// Create dir, pick a non-colliding filename, write. Returns the final path.
fn write_transcript(
    dir: &std::path::Path,
    stem: &str,
    content: &str,
) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;
    let path = unique_md_path(dir, stem);
    std::fs::write(&path, content).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
    Ok(path)
}

/// Create the in-progress meeting-notes file at session start, falling back
/// to the default dir if the configured one is unwritable.
fn create_live_file(app: &AppHandle, started: SystemTime) -> Option<std::path::PathBuf> {
    let started: chrono::DateTime<chrono::Local> = started.into();
    // The live name always carries the time for uniqueness; the FINAL name
    // honors the include-time setting at save.
    let stem = format!("{} - Meeting in progress", started.format("%Y-%m-%d %H%M"));
    let md = render_markdown("Meeting in progress", None, started, &[]);

    let dir = transcript_save_dir(app);
    match write_transcript(&dir, &stem, &md) {
        Ok(path) => {
            eprintln!("[AiBuddy] live notes: {}", path.display());
            Some(path)
        }
        Err(first_err) => {
            let default_dir = expand_home(DEFAULT_TRANSCRIPT_DIR);
            if default_dir != dir {
                eprintln!("[AiBuddy] live notes: {first_err} — falling back to {}", default_dir.display());
                write_transcript(&default_dir, &stem, &md).ok()
            } else {
                eprintln!("[AiBuddy] live notes creation failed: {first_err}");
                None
            }
        }
    }
}

/// On failure the transcript store is left untouched — the transcript stays
/// visible in the UI so the user can copy it manually.
fn save_transcript(app: &AppHandle) {
    let store = app.state::<TranscriptStore>();
    let mut segments: Vec<StoredSegment> = match store.segments.lock() {
        Ok(s) => s.clone(),
        Err(_) => return,
    };
    let live_path = store.live_path.lock().ok().and_then(|mut g| g.take());
    // The recorded Them stream, consumed (and always deleted) by diarization.
    let wav = them_session_wav_path(app);

    if segments.is_empty() {
        eprintln!("[AiBuddy] transcript save: store empty — nothing to write");
        // The live file was created at start but holds nothing — clean it up.
        if let Some(p) = live_path {
            std::fs::remove_file(p).ok();
        }
        if let Some(w) = &wav {
            std::fs::remove_file(w).ok();
        }
        app.emit("transcript-discarded", ()).ok();
        return;
    }

    // Speaker diarization: relabel "them" segments as Speaker 1/2/3 from the
    // recorded audio. Best-effort — failures leave the "Them" labels intact.
    if let Some(w) = &wav {
        if w.exists() {
            let t0 = std::time::Instant::now();
            match crate::diarization::diarize(app, w) {
                Ok(speakers) => {
                    eprintln!(
                        "[AiBuddy] diarization: {} speaker-segment(s) in {:.1}s",
                        speakers.len(),
                        t0.elapsed().as_secs_f64()
                    );
                    apply_diarization(&mut segments, &speakers);
                }
                Err(e) => eprintln!("[AiBuddy] diarization failed: {e}"),
            }
            std::fs::remove_file(w).ok();
        }
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

    let summary = generate_summary(app, &segments);
    eprintln!("[AiBuddy] transcript save: summary = {} chars", summary.len());

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

    let summary_opt = (!summary.trim().is_empty()).then_some(summary.as_str());
    let md = render_markdown(&subject, summary_opt, started, &segments);

    // Preferred path: finalize the live file in place (write real subject,
    // rename to the real name). Falls back to a fresh write if there is no
    // live file or finalizing it fails.
    let result = match &live_path {
        Some(live) if live.exists() => {
            let final_path = unique_md_path(live.parent().unwrap_or(std::path::Path::new(".")), &stem);
            std::fs::write(live, &md)
                .map_err(|e| format!("couldn't write {}: {e}", live.display()))
                .and_then(|_| {
                    std::fs::rename(live, &final_path)
                        .map(|_| final_path)
                        .map_err(|e| format!("couldn't rename {}: {e}", live.display()))
                })
        }
        _ => Err("no live notes file".to_string()),
    }
    .or_else(|live_err| {
        eprintln!("[AiBuddy] transcript save: {live_err} — writing fresh");
        let dir = transcript_save_dir(app);
        write_transcript(&dir, &stem, &md).or_else(|first_err| {
            // Configured dir failed — fall back to the default location.
            let default_dir = expand_home(DEFAULT_TRANSCRIPT_DIR);
            if default_dir != dir {
                eprintln!("[AiBuddy] transcript save: {first_err} — falling back to {}", default_dir.display());
                write_transcript(&default_dir, &stem, &md)
                    .map_err(|e2| format!("{first_err}; fallback also failed: {e2}"))
            } else {
                Err(first_err)
            }
        })
    });

    match result {
        Ok(path) => {
            eprintln!("[AiBuddy] transcript saved: {}", path.display());
            if let Ok(mut last) = store.last_saved.lock() {
                *last = Some(path.clone());
            }
            app.emit("transcript-saved", path.to_string_lossy().to_string()).ok();
        }
        Err(e) => {
            eprintln!("[AiBuddy] transcript save FAILED: {e}");
            app.emit("transcript-save-failed", e).ok();
        }
    }
}
