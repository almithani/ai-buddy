//! SpeechAnalyzer asset (on-device model) status and installation (macOS 26+).

use tauri::Emitter;

#[cfg(target_os = "macos")]
use std::ffi::c_void;

#[derive(Clone, serde::Serialize)]
pub struct AssetProgress {
    pub progress: f64,
    pub done: bool,
    pub error: Option<String>,
}

#[cfg(target_os = "macos")]
extern "C" {
    /// 0 unsupported, 1 installed, 2 download required, -1 pre-macOS-26.
    fn aibuddy_sa_assets_status() -> i32;
    fn aibuddy_sa_assets_install(
        progress_cb: extern "C" fn(f64, *mut c_void),
        done_cb: extern "C" fn(i32, *mut c_void),
        ctx: *mut c_void,
    );
}

#[tauri::command]
pub fn speech_assets_status() -> String {
    #[cfg(target_os = "macos")]
    {
        match unsafe { aibuddy_sa_assets_status() } {
            1 => "installed",
            2 => "download-required",
            -1 => "legacy", // pre-macOS-26: SFSpeechRecognizer path, no assets needed
            _ => "unsupported",
        }
        .to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        "unsupported".to_string()
    }
}

#[cfg(target_os = "macos")]
struct InstallCtx {
    app: tauri::AppHandle,
    done_tx: std::sync::mpsc::Sender<i32>,
}

#[cfg(target_os = "macos")]
extern "C" fn on_asset_progress(percent: f64, ctx: *mut c_void) {
    let ctx = unsafe { &*(ctx as *const InstallCtx) };
    ctx.app
        .emit(
            "speech-assets-progress",
            AssetProgress { progress: percent, done: false, error: None },
        )
        .ok();
}

#[cfg(target_os = "macos")]
extern "C" fn on_asset_done(rc: i32, ctx: *mut c_void) {
    // Reclaim the box — this callback fires exactly once.
    let ctx = unsafe { Box::from_raw(ctx as *mut InstallCtx) };
    let error = match rc {
        0 => None,
        -1 => Some("Requires macOS 26 or later".to_string()),
        -2 => Some("No supported speech locale".to_string()),
        _ => Some("Speech model download failed".to_string()),
    };
    ctx.app
        .emit(
            "speech-assets-progress",
            AssetProgress { progress: 100.0, done: true, error: error.clone() },
        )
        .ok();
    ctx.done_tx.send(rc).ok();
}

#[tauri::command]
pub async fn install_speech_assets(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::channel::<i32>();
        let ctx = Box::into_raw(Box::new(InstallCtx { app, done_tx: tx })) as *mut c_void;
        unsafe { aibuddy_sa_assets_install(on_asset_progress, on_asset_done, ctx) };

        let rc = tauri::async_runtime::spawn_blocking(move || {
            rx.recv_timeout(std::time::Duration::from_secs(600))
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|_| "Timed out downloading speech model".to_string())?;

        if rc == 0 {
            Ok(())
        } else {
            Err(format!("Speech model install failed (code {rc})"))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("Speech assets are only supported on macOS".to_string())
    }
}
