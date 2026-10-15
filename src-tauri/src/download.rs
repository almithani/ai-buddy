use futures_util::StreamExt;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};

/// One downloadable Gemma 4 build (unsloth Q4_K_M — no HuggingFace login required).
pub struct ModelChoice {
    /// Shown on the onboarding download screen.
    pub name: &'static str,
    url: &'static str,
    pub filename: &'static str,
    /// Download size shown in onboarding.
    pub size_gb: f64,
}

const GEMMA_E4B: ModelChoice = ModelChoice {
    name: "Gemma 4 E4B",
    url: "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_K_M.gguf",
    filename: "gemma-4-E4B-it-Q4_K_M.gguf",
    size_gb: 5.0,
};

const GEMMA_E2B: ModelChoice = ModelChoice {
    name: "Gemma 4 E2B",
    url: "https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/gemma-4-E2B-it-Q4_K_M.gguf",
    filename: "gemma-4-E2B-it-Q4_K_M.gguf",
    size_gb: 3.1,
};

/// Macs below this much RAM get the smaller model. Measured: E4B needs ~5.76 GB
/// on the GPU (model 4.73 GiB + KV 0.22 GiB + compute 0.54 GiB), but macOS caps
/// GPU memory at roughly 2/3 of RAM (~5.4 GB on an 8 GB Mac) — the first decode
/// failed there with "Decode Error -3". E2B (3.1 GB) fits with room to spare.
const SMALL_MODEL_BELOW_RAM: u64 = 12 * 1024 * 1024 * 1024;

fn total_ram_bytes() -> u64 {
    #[cfg(target_os = "macos")]
    {
        let mut size: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        let ok = unsafe {
            libc::sysctlbyname(
                c"hw.memsize".as_ptr(),
                &mut size as *mut u64 as *mut libc::c_void,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } == 0;
        if ok {
            return size;
        }
    }
    u64::MAX
}

/// The model for this Mac. `AIBUDDY_MODEL=e2b|e4b` overrides it (for testing).
pub fn model_choice() -> &'static ModelChoice {
    match std::env::var("AIBUDDY_MODEL").as_deref() {
        Ok("e2b") => return &GEMMA_E2B,
        Ok("e4b") => return &GEMMA_E4B,
        _ => {}
    }
    choice_for_ram(total_ram_bytes())
}

fn choice_for_ram(ram_bytes: u64) -> &'static ModelChoice {
    if ram_bytes < SMALL_MODEL_BELOW_RAM { &GEMMA_E2B } else { &GEMMA_E4B }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    name: String,
    filename: String,
    size_gb: f64,
}

#[tauri::command]
pub fn model_info() -> ModelInfo {
    let m = model_choice();
    ModelInfo { name: m.name.to_string(), filename: m.filename.to_string(), size_gb: m.size_gb }
}

#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    pub progress: f64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub done: bool,
    pub error: Option<String>,
}

pub fn model_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    // Prefer the bundled copy inside the .app (or src-tauri/resources/ in dev).
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("models").join(model_choice().filename);
        if bundled.exists() {
            return Ok(bundled);
        }
    }
    // Fall back to a runtime-downloaded copy in app data.
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("models").join(model_choice().filename))
}

#[tauri::command]
pub fn check_model_exists(app: tauri::AppHandle) -> bool {
    model_path(&app).map(|p| p.exists()).unwrap_or(false)
}

/// Set while a download is running. A second `start_model_download` (React
/// StrictMode runs the onboarding effect twice in dev) used to start a second
/// download into the same `.part` file: progress jumped between the two streams
/// and the finished file could be a corrupt mix of both.
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

struct DownloadGuard;

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        DOWNLOADING.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn start_model_download(app: tauri::AppHandle) -> Result<(), String> {
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        // Already running — its progress events reach every listener.
        return Ok(());
    }
    let _guard = DownloadGuard;
    let dest = model_path(&app)?;

    // Already downloaded
    if dest.exists() {
        app.emit(
            "model-download-progress",
            DownloadProgress { progress: 100.0, downloaded_bytes: 0, total_bytes: 0, done: true, error: None },
        )
        .ok();
        return Ok(());
    }

    // Create models directory
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }

    let client = reqwest::Client::builder()
        .user_agent("aibuddy/0.1")
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(model_choice().url).send().await.map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }

    let total_bytes = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let tmp = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp).await.map_err(|e| e.to_string())?;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| e.to_string())?;

        let progress = if total_bytes > 0 {
            (downloaded as f64 / total_bytes as f64) * 100.0
        } else {
            0.0
        };

        app.emit(
            "model-download-progress",
            DownloadProgress {
                progress,
                downloaded_bytes: downloaded,
                total_bytes,
                done: false,
                error: None,
            },
        )
        .ok();
    }

    // Only keep a complete download: a short read (dropped connection) must not
    // become the model file.
    tokio::io::AsyncWriteExt::flush(&mut file).await.map_err(|e| e.to_string())?;
    drop(file);
    if total_bytes > 0 && downloaded != total_bytes {
        tokio::fs::remove_file(&tmp).await.ok();
        return Err(format!("Download incomplete ({downloaded} of {total_bytes} bytes) — please try again."));
    }

    // Atomic rename: only replace the final file once fully written
    tokio::fs::rename(&tmp, &dest).await.map_err(|e| e.to_string())?;

    // The other Gemma build is no longer used on this Mac (e.g. an 8 GB Mac that
    // first downloaded the big one) — free its ~3–5 GB. Not when the model was
    // forced with AIBUDDY_MODEL (testing), so a dev's real model isn't deleted.
    if let (Some(dir), Err(_)) = (dest.parent(), std::env::var("AIBUDDY_MODEL")) {
        for other in [&GEMMA_E4B, &GEMMA_E2B] {
            if other.filename != model_choice().filename {
                tokio::fs::remove_file(dir.join(other.filename)).await.ok();
            }
        }
    }

    app.emit(
        "model-download-progress",
        DownloadProgress {
            progress: 100.0,
            downloaded_bytes: downloaded,
            total_bytes: downloaded,
            done: true,
            error: None,
        },
    )
    .ok();

    Ok(())
}

#[tauri::command]
pub async fn cancel_model_download(app: tauri::AppHandle) -> Result<(), String> {
    // Remove partial file if present
    if let Ok(dest) = model_path(&app) {
        let tmp = dest.with_extension("part");
        tokio::fs::remove_file(&tmp).await.ok();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn eight_gb_macs_get_the_small_model() {
        assert_eq!(choice_for_ram(8 * GIB).filename, GEMMA_E2B.filename);
    }

    #[test]
    fn sixteen_gb_and_up_keep_the_full_model() {
        for gb in [16, 18, 24, 32, 64] {
            assert_eq!(choice_for_ram(gb * GIB).filename, GEMMA_E4B.filename, "{gb} GB");
        }
    }
}
