use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub const WHISPER_FILENAME: &str = "ggml-base.en.bin";

// ── Tauri managed states ──────────────────────────────────────────────────────

pub struct WhisperState(pub Mutex<Option<WhisperContext>>);

/// None = idle, Some(stop_flag) = actively capturing.
pub struct TranscriptionHandle(pub Mutex<Option<Arc<AtomicBool>>>);

// ── Model path ────────────────────────────────────────────────────────────────

pub fn whisper_model_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("models").join(WHISPER_FILENAME))
}

// ── Phase 1: model loading commands ──────────────────────────────────────────

#[tauri::command]
pub fn check_whisper_exists(app: tauri::AppHandle) -> bool {
    whisper_model_path(&app).map(|p| p.exists()).unwrap_or(false)
}

#[tauri::command]
pub fn is_whisper_loaded(state: tauri::State<'_, WhisperState>) -> bool {
    state.0.lock().map(|g| g.is_some()).unwrap_or(false)
}

#[tauri::command]
pub fn load_whisper(
    app: tauri::AppHandle,
    state: tauri::State<'_, WhisperState>,
) -> Result<(), String> {
    let path = whisper_model_path(&app)?;
    if !path.exists() {
        return Err("Whisper model not downloaded yet".to_string());
    }
    let mut params = WhisperContextParameters::default();
    params.use_gpu(true);
    let ctx = WhisperContext::new_with_params(
        path.to_str().ok_or("invalid model path")?,
        params,
    )
    .map_err(|e| format!("Failed to load Whisper: {e}"))?;
    *state.0.lock().map_err(|e| e.to_string())? = Some(ctx);
    Ok(())
}

// ── Phase 2: audio capture + transcription ────────────────────────────────────

#[derive(Clone, serde::Serialize)]
pub struct TranscriptionSegment {
    pub text: String,
    pub done: bool,
}

/// Heap-allocated context passed as *mut c_void to the ObjC audio callback.
/// Intentionally leaked — it's tiny and there's at most one per session.
struct CaptureCtx {
    sender: std::sync::mpsc::SyncSender<Vec<f32>>,
    stop: Arc<AtomicBool>,
}

// ── ObjC extern declarations (macOS only) ────────────────────────────────────

#[cfg(target_os = "macos")]
extern "C" {
    /// Returns 1 on macOS 13+, 0 if unavailable.
    fn aibuddy_start_capture(
        callback: extern "C" fn(*const f32, usize, *mut std::ffi::c_void),
        ctx: *mut std::ffi::c_void,
    ) -> i32;
    fn aibuddy_stop_capture();
}

/// Audio callback fired by the ObjC SCStreamOutput delegate.
extern "C" fn on_audio_samples(
    samples: *const f32,
    count: usize,
    ctx_ptr: *mut std::ffi::c_void,
) {
    let ctx = unsafe { &*(ctx_ptr as *const CaptureCtx) };
    if ctx.stop.load(Ordering::Relaxed) {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts(samples, count) };
    // try_send: if the Rust processing loop is busy, drop rather than block the audio thread.
    let _ = ctx.sender.try_send(slice.to_vec());
}

/// Accumulates audio chunks and runs Whisper every WINDOW_SAMPLES samples.
/// Returns the WhisperContext so the caller can put it back in WhisperState.
fn process_audio(
    rx: std::sync::mpsc::Receiver<Vec<f32>>,
    ctx: WhisperContext,
    app: tauri::AppHandle,
    stop: Arc<AtomicBool>,
) -> WhisperContext {
    const WINDOW_SAMPLES: usize = 5 * 16_000; // 5 seconds at 16 kHz mono

    let mut accumulated = Vec::<f32>::with_capacity(WINDOW_SAMPLES * 2);

    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(chunk) => {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                accumulated.extend_from_slice(&chunk);

                while accumulated.len() >= WINDOW_SAMPLES {
                    let window: Vec<f32> = accumulated.drain(..WINDOW_SAMPLES).collect();
                    run_whisper_on_window(&ctx, &window, &app);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    // Flush any remaining audio (<5 s) if substantial enough for Whisper (~0.5 s minimum).
    if accumulated.len() >= 8_000 {
        run_whisper_on_window(&ctx, &accumulated, &app);
    }

    app.emit(
        "transcription-segment",
        TranscriptionSegment { text: String::new(), done: true },
    )
    .ok();

    ctx
}

fn is_silence(samples: &[f32]) -> bool {
    if samples.is_empty() {
        return true;
    }
    let rms = (samples.iter().map(|&s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    rms < 0.01
}

fn run_whisper_on_window(ctx: &WhisperContext, samples: &[f32], app: &tauri::AppHandle) {
    if is_silence(samples) {
        return;
    }

    let mut state = match ctx.create_state() {
        Ok(s) => s,
        Err(_) => return,
    };

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_translate(false);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    params.set_n_threads(
        std::thread::available_parallelism()
            .map(|n| n.get() as i32)
            .unwrap_or(4)
            .min(8),
    );

    if state.full(params, samples).is_err() {
        return;
    }

    let n = match state.full_n_segments() {
        Ok(n) => n,
        Err(_) => return,
    };

    let mut text = String::new();
    for i in 0..n {
        if let Ok(seg) = state.full_get_segment_text(i) {
            let s = seg.trim();
            if s.is_empty() || s == "[BLANK_AUDIO]" || s == "(silence)" {
                continue;
            }
            text.push_str(s);
            text.push(' ');
        }
    }

    let text = text.trim().to_string();
    if !text.is_empty() {
        app.emit(
            "transcription-segment",
            TranscriptionSegment { text, done: false },
        )
        .ok();
    }
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn is_transcribing(handle: tauri::State<'_, TranscriptionHandle>) -> bool {
    handle.0.lock().map(|g| g.is_some()).unwrap_or(false)
}

#[tauri::command]
pub fn start_transcription(
    app: tauri::AppHandle,
    whisper_state: tauri::State<'_, WhisperState>,
    handle: tauri::State<'_, TranscriptionHandle>,
) -> Result<(), String> {
    let mut h = handle.0.lock().map_err(|e| e.to_string())?;
    if h.is_some() {
        return Err("Transcription is already running — stop it first".to_string());
    }

    // Take the WhisperContext out of the shared state for exclusive use during capture.
    let ctx = {
        let mut ws = whisper_state.0.lock().map_err(|e| e.to_string())?;
        ws.take()
            .ok_or("Whisper model not loaded — please download and load it first")?
    };

    let stop = Arc::new(AtomicBool::new(false));
    *h = Some(stop.clone());
    drop(h); // release lock before spawning

    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(20);

    // Leak the capture context — it's tiny and lives for the capture session.
    let ctx_ptr = Box::into_raw(Box::new(CaptureCtx {
        sender: tx,
        stop: stop.clone(),
    })) as usize;

    #[cfg(target_os = "macos")]
    {
        let started = unsafe {
            aibuddy_start_capture(on_audio_samples, ctx_ptr as *mut std::ffi::c_void)
        };
        if started == 0 {
            // Clean up: return ctx to state and clear handle before returning error.
            *whisper_state.0.lock().map_err(|e| e.to_string())? = Some(ctx);
            *handle.0.lock().map_err(|e| e.to_string())? = None;
            return Err("Audio capture requires macOS 13.0 or later".to_string());
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        *whisper_state.0.lock().map_err(|e| e.to_string())? = Some(ctx);
        *handle.0.lock().map_err(|e| e.to_string())? = None;
        return Err("Audio capture is only supported on macOS".to_string());
    }

    // Safety: both Tauri managed states outlive any spawned task.
    let ws_raw = &*whisper_state as *const WhisperState as usize;
    let handle_raw = &*handle as *const TranscriptionHandle as usize;

    std::thread::spawn(move || {
        let returned = process_audio(rx, ctx, app, stop);

        let ws = unsafe { &*(ws_raw as *const WhisperState) };
        *ws.0.lock().unwrap() = Some(returned);

        let h = unsafe { &*(handle_raw as *const TranscriptionHandle) };
        *h.0.lock().unwrap() = None;
    });

    Ok(())
}

#[tauri::command]
pub fn stop_transcription(
    handle: tauri::State<'_, TranscriptionHandle>,
) -> Result<(), String> {
    let stop = handle.0.lock().map_err(|e| e.to_string())?.take();
    if let Some(flag) = stop {
        flag.store(true, Ordering::SeqCst);
        #[cfg(target_os = "macos")]
        unsafe {
            aibuddy_stop_capture();
        }
    }
    Ok(())
}
