use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, Manager};

#[cfg(target_os = "macos")]
use std::ffi::{c_char, c_void, CStr};

/// Managed state: true while a transcription session is running.
pub struct TranscriptionActive(pub AtomicBool);

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
                state.0.store(true, Ordering::SeqCst);
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
pub fn stop_transcription(state: tauri::State<'_, TranscriptionActive>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    unsafe {
        aibuddy_speech_stop();
    }
    state.0.store(false, Ordering::SeqCst);
    Ok(())
}
