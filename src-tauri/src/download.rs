use futures_util::StreamExt;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{Emitter, Manager};

/// Gemma 4 Q4_K_M from unsloth — ~5 GB, no HuggingFace login required.
const MODEL_URL: &str =
    "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_K_M.gguf";

pub const MODEL_FILENAME: &str = "gemma-4-E4B-it-Q4_K_M.gguf";

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
        let bundled = resource_dir.join("models").join(MODEL_FILENAME);
        if bundled.exists() {
            return Ok(bundled);
        }
    }
    // Fall back to a runtime-downloaded copy in app data.
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("models").join(MODEL_FILENAME))
}

#[tauri::command]
pub fn check_model_exists(app: tauri::AppHandle) -> bool {
    model_path(&app).map(|p| p.exists()).unwrap_or(false)
}

#[tauri::command]
pub async fn start_model_download(app: tauri::AppHandle) -> Result<(), String> {
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

    let response = client.get(MODEL_URL).send().await.map_err(|e| e.to_string())?;

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

    // Atomic rename: only replace the final file once fully written
    tokio::fs::rename(&tmp, &dest).await.map_err(|e| e.to_string())?;

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
