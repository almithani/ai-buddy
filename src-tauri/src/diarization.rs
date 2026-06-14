//! Local speaker diarization for the "Them" stream, via sherpa-onnx
//! (pyannote segmentation + 3D-Speaker CAM++ embeddings). Models download
//! once on demand; diarization runs post-meeting in `save_transcript`.

use futures_util::StreamExt;
use std::path::PathBuf;
use tauri::{Emitter, Manager};

const SEG_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2";
const EMB_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx";

const SEG_FILE: &str = "segmentation.onnx";
const EMB_FILE: &str = "embedding.onnx";

#[derive(Clone, serde::Serialize)]
pub struct DiarizeProgress {
    pub progress: f64,
    pub done: bool,
    pub error: Option<String>,
}

fn model_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("models").join("diarization"))
}

pub fn seg_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    model_dir(app).map(|d| d.join(SEG_FILE))
}

pub fn emb_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    model_dir(app).map(|d| d.join(EMB_FILE))
}

pub fn models_installed(app: &tauri::AppHandle) -> bool {
    matches!((seg_path(app), emb_path(app)), (Some(s), Some(e)) if s.exists() && e.exists())
}

#[tauri::command]
pub fn diarization_models_status(app: tauri::AppHandle) -> String {
    if models_installed(&app) { "installed" } else { "missing" }.to_string()
}

async fn download_to(
    client: &reqwest::Client,
    url: &str,
    dest: &std::path::Path,
    app: &tauri::AppHandle,
    base: f64,
    span: f64,
) -> Result<(), String> {
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let tmp = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp).await.map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| e.to_string())?;
        let frac = if total > 0 { downloaded as f64 / total as f64 } else { 0.0 };
        app.emit(
            "diarization-download-progress",
            DiarizeProgress { progress: base + frac * span, done: false, error: None },
        )
        .ok();
    }
    drop(file);
    tokio::fs::rename(&tmp, dest).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn install_diarization_models(app: tauri::AppHandle) -> Result<(), String> {
    if models_installed(&app) {
        app.emit(
            "diarization-download-progress",
            DiarizeProgress { progress: 100.0, done: true, error: None },
        )
        .ok();
        return Ok(());
    }

    let dir = model_dir(&app).ok_or("no app data dir")?;
    tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;

    let result: Result<(), String> = async {
        let client = reqwest::Client::builder()
            .user_agent("aibuddy/0.1")
            .build()
            .map_err(|e| e.to_string())?;

        // Embedding model: direct .onnx (the larger file → first 70% of the bar).
        let emb_dest = dir.join(EMB_FILE);
        if !emb_dest.exists() {
            download_to(&client, EMB_URL, &emb_dest, &app, 0.0, 70.0).await?;
        }

        // Segmentation model: tar.bz2 containing model.onnx (next 30%).
        let seg_dest = dir.join(SEG_FILE);
        if !seg_dest.exists() {
            let tar = dir.join("seg.tar.bz2");
            download_to(&client, SEG_URL, &tar, &app, 70.0, 30.0).await?;
            extract_segmentation(&tar, &seg_dest)?;
            tokio::fs::remove_file(&tar).await.ok();
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            app.emit(
                "diarization-download-progress",
                DiarizeProgress { progress: 100.0, done: true, error: None },
            )
            .ok();
            Ok(())
        }
        Err(e) => {
            app.emit(
                "diarization-download-progress",
                DiarizeProgress { progress: 0.0, done: true, error: Some(e.clone()) },
            )
            .ok();
            Err(e)
        }
    }
}

/// Extract `model.onnx` from the pyannote tar.bz2 into `dest`. macOS `tar`
/// handles bzip2 natively, so we shell out rather than pull in bz2 crates.
fn extract_segmentation(tar: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    let tmp = dest
        .parent()
        .ok_or("bad dest")?
        .join("seg-extract");
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let status = std::process::Command::new("tar")
        .arg("xjf")
        .arg(tar)
        .arg("-C")
        .arg(&tmp)
        .status()
        .map_err(|e| format!("tar failed: {e}"))?;
    if !status.success() {
        return Err("tar extraction failed".into());
    }
    // Find model.onnx anywhere under the extraction dir.
    let found = find_file(&tmp, "model.onnx").ok_or("model.onnx not found in archive")?;
    std::fs::rename(&found, dest).map_err(|e| e.to_string())?;
    std::fs::remove_dir_all(&tmp).ok();
    Ok(())
}

fn find_file(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(found) = find_file(&p, name) {
                return Some(found);
            }
        } else if p.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(p);
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct SpeakerSegment {
    pub start: f32,
    pub end: f32,
    pub speaker: i32,
}

/// Run diarization over a recorded WAV. Returns speaker segments sorted by
/// start time. `num_speakers` < 1 → auto-detect via clustering threshold.
pub fn diarize(
    app: &tauri::AppHandle,
    wav: &std::path::Path,
) -> Result<Vec<SpeakerSegment>, String> {
    let seg = seg_path(app).ok_or("no seg model path")?;
    let emb = emb_path(app).ok_or("no emb model path")?;

    let (samples, sample_rate) =
        sherpa_rs::read_audio_file(wav.to_str().ok_or("bad wav path")?).map_err(|e| e.to_string())?;
    if sample_rate != 16000 {
        eprintln!(
            "[AiBuddy] diarization: wav is {sample_rate} Hz (expected 16000) — results may degrade"
        );
    }
    if samples.is_empty() {
        return Ok(Vec::new());
    }

    let config = sherpa_rs::diarize::DiarizeConfig {
        num_clusters: Some(-1), // auto-detect speaker count via threshold
        threshold: Some(0.5),
        ..Default::default()
    };
    let mut sd = sherpa_rs::diarize::Diarize::new(&seg, &emb, config).map_err(|e| e.to_string())?;
    let segments = sd.compute(samples, None).map_err(|e| e.to_string())?;

    Ok(segments
        .into_iter()
        .map(|s| SpeakerSegment { start: s.start, end: s.end, speaker: s.speaker })
        .collect())
}
