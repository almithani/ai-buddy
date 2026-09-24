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
    /// Set while the post-Stop save is finalizing (diarization + summary), so a
    /// remounted TranscriptPanel can restore the "finalizing" status bar.
    pub processing: Mutex<Option<ProcessingState>>,
}

#[derive(Clone)]
pub struct ProcessingState {
    pub stage: String,
    pub path: std::path::PathBuf,
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
    fn aibuddy_begin_processing_activity() -> *mut c_void;
    fn aibuddy_end_processing_activity(token: *mut c_void);
}

/// Holds an NSProcessInfo activity assertion so macOS App Nap doesn't throttle
/// the background transcript save when the app isn't frontmost. No-op off macOS.
struct ActivityGuard {
    #[cfg(target_os = "macos")]
    token: *mut c_void,
}

impl ActivityGuard {
    fn begin() -> Self {
        #[cfg(target_os = "macos")]
        {
            ActivityGuard { token: unsafe { aibuddy_begin_processing_activity() } }
        }
        #[cfg(not(target_os = "macos"))]
        {
            ActivityGuard {}
        }
    }
}

impl Drop for ActivityGuard {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        unsafe {
            aibuddy_end_processing_activity(self.token);
        }
    }
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
    let md = render_markdown("Meeting in progress", SummarySection::Pending, started, &segments);
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
    /// In-progress finalize stage + its working file (set during the save).
    pub processing_stage: Option<String>,
    pub processing_path: Option<String>,
}

#[tauri::command]
pub fn get_transcript_files(store: tauri::State<'_, TranscriptStore>) -> TranscriptFiles {
    let path_str = |g: &Mutex<Option<std::path::PathBuf>>| {
        g.lock()
            .ok()
            .and_then(|p| p.as_ref().map(|p| p.to_string_lossy().to_string()))
    };
    let (processing_stage, processing_path) = store
        .processing
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .map(|p| (Some(p.stage), Some(p.path.to_string_lossy().to_string())))
        .unwrap_or((None, None));
    TranscriptFiles {
        live: path_str(&store.live_path),
        saved: path_str(&store.last_saved),
        processing_stage,
        processing_path,
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

/// The final minutes prompt over `body` (either the whole transcript when short,
/// or the concatenated chunk-notes when long).
fn summarize_minutes_prompt(body: &str) -> String {
    format!(
        "You are summarizing a meeting into concise minutes. \
         Write short markdown bullet points under exactly these three headings \
         (omit a heading only if it has nothing):\n\
         **Key Points**\n**Decisions**\n**Action Items**\n\n\
         Be specific and factual. Do not invent content.\n\nMeeting notes:\n{body}"
    )
}

/// Bulleted meeting summary (Key Points / Decisions / Action Items). Returns ""
/// on error or empty input. For long meetings it map-reduces (summarize chunks,
/// then summarize the chunk-notes) so the WHOLE meeting is represented — not
/// just the opening minutes that would survive `generate_text`'s input cap.
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

    let llm = app.state::<crate::llm::LlmState>();

    // Short enough to summarize in one pass (stays under generate_text's cap).
    const ONE_PASS_LIMIT: usize = 9000;
    if transcript.len() <= ONE_PASS_LIMIT {
        return crate::llm::generate_text(&llm, &summarize_minutes_prompt(&transcript), 400)
            .unwrap_or_default();
    }

    // Long meeting → map-reduce. Split into chunks on line boundaries.
    const CHUNK_CHARS: usize = 8000;
    const MAX_CHUNKS: usize = 16; // cap work for extreme meetings
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in transcript.lines() {
        if cur.len() + line.len() + 1 > CHUNK_CHARS && !cur.is_empty() {
            chunks.push(std::mem::take(&mut cur));
            if chunks.len() >= MAX_CHUNKS {
                break;
            }
        }
        cur.push_str(line);
        cur.push('\n');
    }
    if !cur.is_empty() && chunks.len() < MAX_CHUNKS {
        chunks.push(cur);
    }
    eprintln!("[AiBuddy] summary: map-reduce over {} chunk(s)", chunks.len());

    let mut notes = String::new();
    for (i, chunk) in chunks.iter().enumerate() {
        let prompt = format!(
            "Briefly note the key points, decisions, and action items from this \
             part of a meeting transcript, as short bullet points. Be factual.\n\n\
             Transcript part:\n{chunk}"
        );
        if let Ok(part) = crate::llm::generate_text(&llm, &prompt, 200) {
            let part = part.trim();
            if !part.is_empty() {
                notes.push_str(&format!("Part {}:\n{}\n\n", i + 1, part));
            }
        }
    }
    if notes.trim().is_empty() {
        return String::new();
    }

    // Reduce: the chunk-notes are already small, so this fits one pass.
    crate::llm::generate_text(&llm, &summarize_minutes_prompt(notes.trim()), 400)
        .unwrap_or_default()
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

/// State of the AI summary section when rendering the meeting-notes file.
enum SummarySection<'a> {
    /// Live session — summary is produced at the end.
    Pending,
    /// Final summary text.
    Text(&'a str),
    /// Save completed but summarization produced nothing.
    Unavailable,
}

/// Shared renderer for the live file and the final save.
fn render_markdown(
    subject: &str,
    summary: SummarySection,
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
        SummarySection::Text(s) if !s.trim().is_empty() => {
            md.push_str(s.trim());
            md.push_str("\n\n");
        }
        SummarySection::Unavailable => {
            md.push_str("_Summary unavailable for this meeting._\n\n");
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

/// Tokenize for echo comparison: lowercase, keep alphanumerics, split on the rest.
fn normalize_tokens(s: &str) -> Vec<String> {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .map(|w| w.to_string())
        .collect()
}

/// (jaccard, containment) over the two token sets. Containment uses the smaller
/// set as denominator so a short "me" that's a subset of a longer "them" scores high.
fn token_similarity(a: &[String], b: &[String]) -> (f64, f64) {
    use std::collections::HashSet;
    let sa: HashSet<&String> = a.iter().collect();
    let sb: HashSet<&String> = b.iter().collect();
    if sa.is_empty() || sb.is_empty() {
        return (0.0, 0.0);
    }
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count();
    let jaccard = inter as f64 / union as f64;
    let containment = inter as f64 / sa.len().min(sb.len()) as f64;
    (jaccard, containment)
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}

/// Character-level similarity (1 = identical, 0 = totally different). Catches
/// the two recognizers transcribing the same sound slightly differently
/// ("sweaty pumps" vs "sweaty palms", "100th" vs "hundredth").
fn char_similarity(a: &str, b: &str) -> f64 {
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    let max = ac.len().max(bc.len());
    if max == 0 {
        return 1.0;
    }
    1.0 - (levenshtein(&ac, &bc) as f64 / max as f64)
}

/// Drop "me" segments that are echoes of a nearby "them" segment — i.e. the mic
/// re-hearing the participants when the user is on speakers (no headphones).
/// Conservative: only removes a "me" line that closely matches a "them" line at
/// about the same time. Self-gating — with headphones the mic never hears the
/// participants, so nothing matches and nothing is dropped. Only "me" is removed.
fn dedup_echo(segments: &mut Vec<StoredSegment>) {
    struct Them {
        tokens: Vec<String>,
        norm: String,
        start: Option<f64>,
        ts_ms: u64,
    }
    let thems: Vec<Them> = segments
        .iter()
        .filter(|s| s.source == "them")
        .map(|s| {
            let tokens = normalize_tokens(&s.text);
            Them { norm: tokens.join(" "), tokens, start: s.start_sec, ts_ms: s.ts_ms }
        })
        .collect();
    if thems.is_empty() {
        return;
    }

    let before = segments.len();
    segments.retain(|seg| {
        if seg.source != "me" {
            return true;
        }
        let me = normalize_tokens(&seg.text);
        let me_norm = me.join(" ");
        // Guard tiny utterances by length, not token count, so 2-word echoes
        // ("sweaty palms") still qualify but "yes"/"ok" never do.
        if me_norm.len() < 6 {
            return true;
        }
        for t in &thems {
            let close = match (seg.start_sec, t.start) {
                (Some(ms), Some(ts)) => (ms - ts).abs() <= 2.5,
                _ => seg.ts_ms.abs_diff(t.ts_ms) <= 6000,
            };
            if !close {
                continue;
            }
            let (jaccard, containment) = token_similarity(&me, &t.tokens);
            // Word-overlap catches one-word swaps (100th↔hundredth); char-level
            // catches sub-word substitutions (pumps↔palms) the recognizers make.
            if jaccard >= 0.6 || containment >= 0.8 || char_similarity(&me_norm, &t.norm) >= 0.72 {
                return false; // echo of a participant — drop
            }
        }
        true
    });
    let dropped = before - segments.len();
    if dropped > 0 {
        eprintln!("[AiBuddy] echo dedup: dropped {dropped} duplicate \"Me\" segment(s)");
    }
}

/// Beyond this many distinct speakers the run is almost certainly broken
/// over-segmentation (a real meeting rarely exceeds this), so we discard the
/// labels and keep "Them". Generous enough for genuinely large meetings.
const MAX_TRUSTED_SPEAKERS: usize = 24;

/// Replace the "them" source of each segment with a "Speaker N" label by
/// intersecting its audio time range against diarization segments. Segments
/// without a time range, or with no overlap, keep "them". "me" is untouched.
/// Returns `true` if labels were applied (or there was legitimately nothing to
/// label), `false` if the result was discarded as unreliable.
fn apply_diarization(
    segments: &mut [StoredSegment],
    speakers: &[crate::diarization::SpeakerSegment],
) -> bool {
    if speakers.is_empty() {
        return true; // nothing to attribute (e.g. no "them" audio) — not a failure
    }
    // Sanity check: an absurd speaker count means the result is garbage —
    // keep "Them" rather than labelling everyone Speaker 1…57.
    let distinct = {
        let mut ids: Vec<i32> = speakers.iter().map(|s| s.speaker).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    };
    if distinct > MAX_TRUSTED_SPEAKERS {
        eprintln!(
            "[AiBuddy] diarization: {distinct} speakers detected (> {MAX_TRUSTED_SPEAKERS}) — \
             discarding as unreliable, keeping \"Them\""
        );
        return false;
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
    true
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
    let md = render_markdown("Meeting in progress", SummarySection::Pending, started, &[]);

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

/// Update the finalize stage: stores it (so a remounting panel can restore the
/// status bar) and emits `transcript-progress` (for live in-place updates).
/// `None` clears the processing state (save done/failed).
fn set_processing(
    app: &AppHandle,
    store: &TranscriptStore,
    state: Option<(std::path::PathBuf, &str)>,
) {
    if let Ok(mut g) = store.processing.lock() {
        *g = state.as_ref().map(|(path, stage)| ProcessingState {
            stage: stage.to_string(),
            path: path.clone(),
        });
    }
    if let Some((_, stage)) = state {
        app.emit("transcript-progress", stage).ok();
    }
}

/// On failure the transcript store is left untouched — the transcript stays
/// visible in the UI so the user can copy it manually.
fn save_transcript(app: &AppHandle) {
    // Keep this heavy work (diarization + LLM passes) running at full speed even
    // when the app is backgrounded — released on every return path.
    let _activity = ActivityGuard::begin();

    let store = app.state::<TranscriptStore>();
    let mut segments: Vec<StoredSegment> = match store.segments.lock() {
        Ok(s) => s.clone(),
        Err(_) => return,
    };
    let live_path = store.live_path.lock().ok().and_then(|mut g| g.take());
    // The recorded Them stream, consumed (and always deleted) by diarization.
    let wav = them_session_wav_path(app);

    // Mark "finalizing" so a remounted panel restores the status bar (the live
    // path was just taken above, and last_saved isn't set until the end).
    if let Some(p) = &live_path {
        set_processing(app, &store, Some((p.clone(), "Finalizing…")));
    }

    if segments.is_empty() {
        eprintln!("[AiBuddy] transcript save: store empty — nothing to write");
        // The live file was created at start but holds nothing — clean it up.
        if let Some(p) = live_path {
            std::fs::remove_file(p).ok();
        }
        if let Some(w) = &wav {
            std::fs::remove_file(w).ok();
        }
        set_processing(app, &store, None);
        app.emit("transcript-discarded", ()).ok();
        return;
    }

    // Remove mic echoes of participants (speaker bleed) before everything else,
    // so diarization, subject, and summary all see the cleaned transcript.
    dedup_echo(&mut segments);

    // Speaker diarization: relabel "them" segments as Speaker 1/2/3 from the
    // recorded audio. Best-effort — failures leave the "Them" labels intact.
    let mut diarization_failed = false;
    if let Some(w) = &wav {
        if w.exists() {
            if let Some(p) = &live_path {
                set_processing(app, &store, Some((p.clone(), "Identifying speakers…")));
            }
            let t0 = std::time::Instant::now();
            match crate::diarization::diarize(app, w) {
                Ok(speakers) => {
                    eprintln!(
                        "[AiBuddy] diarization: {} speaker-segment(s) in {:.1}s",
                        speakers.len(),
                        t0.elapsed().as_secs_f64()
                    );
                    if !apply_diarization(&mut segments, &speakers) {
                        diarization_failed = true;
                    }
                }
                Err(e) => {
                    eprintln!("[AiBuddy] diarization failed: {e}");
                    diarization_failed = true;
                }
            }
            std::fs::remove_file(w).ok();
        }
    }

    eprintln!("[AiBuddy] transcript save: {} segment(s), generating subject…", segments.len());
    if let Some(p) = &live_path {
        set_processing(app, &store, Some((p.clone(), "Writing summary…")));
    }
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

    let summary_section = if summary.trim().is_empty() {
        SummarySection::Unavailable
    } else {
        SummarySection::Text(&summary)
    };
    let md = render_markdown(&subject, summary_section, started, &segments);

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

    set_processing(app, &store, None);
    match result {
        Ok(path) => {
            eprintln!("[AiBuddy] transcript saved: {}", path.display());
            if let Ok(mut last) = store.last_saved.lock() {
                *last = Some(path.clone());
            }
            if diarization_failed {
                // Tell the user the speakers couldn't be separated this time,
                // rather than silently leaving everyone as "Them".
                app.emit("transcript-speakers-unavailable", ()).ok();
            }
            app.emit("transcript-saved", path.to_string_lossy().to_string()).ok();
        }
        Err(e) => {
            eprintln!("[AiBuddy] transcript save FAILED: {e}");
            app.emit("transcript-save-failed", e).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(source: &str, text: &str, start: f64) -> StoredSegment {
        StoredSegment {
            source: source.into(),
            text: text.into(),
            ts_ms: (start * 1000.0) as u64,
            start_sec: Some(start),
            end_sec: Some(start + 2.0),
        }
    }

    fn sources(segs: &[StoredSegment]) -> Vec<&str> {
        segs.iter().map(|s| s.source.as_str()).collect()
    }

    #[test]
    fn drops_me_echo_of_nearby_them() {
        let mut v = vec![
            seg("them", "the quarterly numbers look strong this month", 10.0),
            seg("me", "the quarterly numbers look strong this month", 10.3), // mic bleed
        ];
        dedup_echo(&mut v);
        assert_eq!(sources(&v), vec!["them"]);
    }

    #[test]
    fn keeps_me_when_far_apart_in_time() {
        let mut v = vec![
            seg("them", "the quarterly numbers look strong this month", 10.0),
            seg("me", "the quarterly numbers look strong this month", 40.0), // 30s later — not echo
        ];
        dedup_echo(&mut v);
        assert_eq!(sources(&v), vec!["them", "me"]);
    }

    #[test]
    fn keeps_short_me_utterances() {
        let mut v = vec![
            seg("them", "yes absolutely i agree", 5.0),
            seg("me", "yes", 5.1), // < 3 tokens — never dropped
        ];
        dedup_echo(&mut v);
        assert_eq!(sources(&v), vec!["them", "me"]);
    }

    #[test]
    fn keeps_genuinely_different_me() {
        let mut v = vec![
            seg("them", "the quarterly numbers look strong this month", 10.0),
            seg("me", "can you share your screen with the deck please", 10.4),
        ];
        dedup_echo(&mut v);
        assert_eq!(sources(&v), vec!["them", "me"]);
    }

    #[test]
    fn drops_me_subset_of_longer_them() {
        let mut v = vec![
            seg("them", "so the quarterly numbers look strong this month overall", 10.0),
            seg("me", "quarterly numbers look strong", 10.5), // subset echo (different segmentation)
        ];
        dedup_echo(&mut v);
        assert_eq!(sources(&v), vec!["them"]);
    }

    #[test]
    fn no_them_means_no_change() {
        let mut v = vec![seg("me", "this is just me talking on my own here", 1.0)];
        dedup_echo(&mut v);
        assert_eq!(sources(&v), vec!["me"]);
    }

    #[test]
    fn drops_me_number_word_variant() {
        // Same audio, different recognition: "100th" vs "hundredth".
        let mut v = vec![
            seg("them", "This is my 100th fire.", 30.0),
            seg("me", "This is my hundredth fire.", 30.2),
        ];
        dedup_echo(&mut v);
        assert_eq!(sources(&v), vec!["them"]);
    }

    #[test]
    fn drops_me_subword_substitution() {
        // Two-word echo with a sub-word mishearing: "palms" vs "pumps".
        let mut v = vec![
            seg("them", "sweaty palms", 12.0),
            seg("me", "Sweaty pumps.", 12.2),
        ];
        dedup_echo(&mut v);
        assert_eq!(sources(&v), vec!["them"]);
    }

    #[test]
    fn keeps_two_word_non_echo() {
        // Short but genuinely different from the nearby them → not dropped.
        let mut v = vec![
            seg("them", "sweaty palms", 12.0),
            seg("me", "totally agree", 12.2),
        ];
        dedup_echo(&mut v);
        assert_eq!(sources(&v), vec!["them", "me"]);
    }
}
