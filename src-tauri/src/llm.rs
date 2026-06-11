#[allow(deprecated)]
use llama_cpp_2::{
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaModel, Special},
    sampling::LlamaSampler,
};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use std::sync::{Mutex, OnceLock};
use tauri::Emitter;

use crate::download::model_path;

// Backend is global and initialised once — llama_cpp_2 panics if init() is called twice.
static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();

pub struct LlmState(pub Mutex<Option<LlamaModel>>);

#[derive(Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Serialize)]
pub struct LlmToken {
    pub text: String,
    pub done: bool,
}

/// Format messages using Gemma's chat template.
pub fn format_prompt(system: &str, messages: &[ChatMessage]) -> String {
    let mut prompt = String::new();
    if !system.is_empty() {
        prompt.push_str("<start_of_turn>system\n");
        prompt.push_str(system);
        prompt.push_str("<end_of_turn>\n");
    }
    for msg in messages {
        let role = match msg.role.as_str() {
            "user" => "user",
            _ => "model",
        };
        prompt.push_str(&format!(
            "<start_of_turn>{}\n{}<end_of_turn>\n",
            role, msg.content
        ));
    }
    // Open the model's turn for completion
    prompt.push_str("<start_of_turn>model\n");
    prompt
}

#[tauri::command]
pub fn load_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, LlmState>,
) -> Result<(), String> {
    let path = model_path(&app)?;
    if !path.exists() {
        return Err("Model file not found — complete the download first.".into());
    }

    let backend = BACKEND.get_or_init(|| {
        LlamaBackend::init().expect("Failed to initialise llama.cpp backend")
    });

    // Use all GPU layers on Apple Silicon; falls back to CPU if Metal unavailable.
    let model_params = LlamaModelParams::default().with_n_gpu_layers(9999);
    let model = LlamaModel::load_from_file(backend, &path, &model_params)
        .map_err(|e| format!("Failed to load model: {e}"))?;

    *state.0.lock().map_err(|e| e.to_string())? = Some(model);
    Ok(())
}

#[tauri::command]
pub fn is_model_loaded(state: tauri::State<'_, LlmState>) -> bool {
    state.0.lock().map(|g| g.is_some()).unwrap_or(false)
}

/// Short, synchronous, non-streaming completion for internal use (e.g.
/// transcript subject lines). Holds the model lock for the duration — callers
/// should keep `max_new` small. Does NOT emit llm-token events.
pub fn generate_short_text(
    state: &LlmState,
    prompt_text: &str,
    max_new: u32,
) -> Result<String, String> {
    let backend = BACKEND.get().ok_or("LLM backend not initialised")?;

    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let model = guard.as_ref().ok_or("Model not loaded")?;

    let messages = [ChatMessage { role: "user".into(), content: prompt_text.into() }];
    let prompt = format_prompt("", &messages);

    let tokens: Vec<_> = model
        .str_to_token(&prompt, AddBos::Always)
        .map_err(|e| format!("Tokenisation failed: {e}"))?;

    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(Some(NonZeroU32::new(2048).unwrap()))
        .with_n_threads(
            std::thread::available_parallelism()
                .map(|n| n.get() as i32)
                .unwrap_or(4),
        );
    if tokens.len() + max_new as usize > 2048 {
        return Err("Prompt too long for subject generation".into());
    }

    let mut ctx = model
        .new_context(backend, ctx_params)
        .map_err(|e| format!("Context creation failed: {e}"))?;

    let mut batch = LlamaBatch::new(tokens.len(), 1);
    let last_idx = (tokens.len() as i32) - 1;
    for (i, &tok) in tokens.iter().enumerate() {
        batch.add(tok, i as i32, &[0], i as i32 == last_idx)
            .map_err(|e| format!("Batch add failed: {e}"))?;
    }
    ctx.decode(&mut batch).map_err(|e| format!("Initial decode failed: {e}"))?;

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::temp(0.3),
        LlamaSampler::dist(42),
    ]);

    let mut out = String::new();
    let mut n_pos = tokens.len() as i32;
    for _ in 0..max_new {
        let new_token = sampler.sample(&ctx, -1);
        sampler.accept(new_token);
        if model.is_eog_token(new_token) {
            break;
        }
        #[allow(deprecated)]
        let piece = model.token_to_str(new_token, Special::Tokenize).unwrap_or_default();
        out.push_str(&piece);
        if out.contains("<end_of_turn>") || out.contains("<start_of_turn>") || out.contains('\n') {
            break;
        }
        batch.clear();
        batch.add(new_token, n_pos, &[0], true)
            .map_err(|e| format!("Token batch add failed: {e}"))?;
        ctx.decode(&mut batch).map_err(|e| format!("Token decode failed: {e}"))?;
        n_pos += 1;
    }

    let out = out
        .split("<end_of_turn>").next().unwrap_or("")
        .split("<start_of_turn>").next().unwrap_or("")
        .lines().next().unwrap_or("")
        .trim()
        .to_string();
    Ok(out)
}

#[tauri::command]
pub async fn generate_response(
    app: tauri::AppHandle,
    state: tauri::State<'_, LlmState>,
    messages: Vec<ChatMessage>,
    system_prompt: String,
    max_tokens: Option<u32>,
) -> Result<(), String> {
    let backend = BACKEND
        .get()
        .ok_or("LLM backend not initialised — call load_model first")?;

    let prompt = format_prompt(&system_prompt, &messages);
    let max_new = max_tokens.unwrap_or(512);
    let app_clone = app.clone();

    // Scope the mutex guard so it's dropped before the .await below.
    // MutexGuard is !Send, so it cannot be held across an await point.
    let (model_ptr, tokens, n_input, ctx_params) = {
        let model_guard = state.0.lock().map_err(|e| e.to_string())?;
        let model = model_guard
            .as_ref()
            .ok_or("Model not loaded — call load_model first")?;

        let tokens: Vec<_> = model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|e| format!("Tokenisation failed: {e}"))?;
        let n_input = tokens.len();
        const N_CTX: usize = 4096;
        if n_input + max_new as usize > N_CTX {
            return Err(format!(
                "Input is too long ({n_input} tokens). Shorten the file or message so the total fits in the {N_CTX}-token context window."
            ));
        }

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(4096).unwrap()))
            .with_n_threads(
                std::thread::available_parallelism()
                    .map(|n| n.get() as i32)
                    .unwrap_or(4),
            );

        // Safety: model is pinned in LlmState (Tauri managed state) which
        // outlives any spawned task. We pass it as a raw usize to cross the
        // Send boundary; the blocking closure reconstitutes the reference.
        let model_ptr = model as *const LlamaModel as usize;

        (model_ptr, tokens, n_input, ctx_params)
        // model_guard dropped here — safe to .await after this point
    };

    tokio::task::spawn_blocking(move || {
        // SAFETY: model is pinned in LlmState which outlives this task.
        let model = unsafe { &*(model_ptr as *const LlamaModel) };

        let mut ctx = model
            .new_context(backend, ctx_params)
            .map_err(|e| format!("Context creation failed: {e}"))?;

        let mut batch = LlamaBatch::new(tokens.len(), 1);
        let last_idx = (tokens.len() as i32) - 1;
        for (i, &tok) in tokens.iter().enumerate() {
            batch.add(tok, i as i32, &[0], i as i32 == last_idx)
                .map_err(|e| format!("Batch add failed: {e}"))?;
        }
        ctx.decode(&mut batch).map_err(|e| format!("Initial decode failed: {e}"))?;

        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(0.7),
            LlamaSampler::dist(42),
        ]);

        let mut n_pos = n_input as i32;

        // Rolling buffer for cross-token stop-sequence detection.
        // The model may emit "<end_of_turn>" as individual character tokens
        // rather than the single special token, so a per-token check is not
        // enough — we need to watch the accumulated tail.
        const STOP_SEQS: &[&str] = &["<end_of_turn>", "<start_of_turn>"];
        let max_stop_len = STOP_SEQS.iter().map(|s| s.len()).max().unwrap_or(20);
        let mut tail = String::new();

        for _ in 0..max_new {
            // -1 means "last logit in the batch output", independent of sequence position.
            let new_token = sampler.sample(&ctx, -1);
            sampler.accept(new_token);

            if model.is_eog_token(new_token) {
                break;
            }

            #[allow(deprecated)]
            let piece = model
                .token_to_str(new_token, Special::Tokenize)
                .unwrap_or_default();

            tail.push_str(&piece);

            // Check whether any stop sequence has appeared in the tail.
            if let Some(stop_pos) = STOP_SEQS.iter().filter_map(|s| tail.find(s)).min() {
                if stop_pos > 0 {
                    app_clone
                        .emit("llm-token", LlmToken { text: tail[..stop_pos].to_string(), done: false })
                        .ok();
                }
                break;
            }

            // Emit the safe prefix — everything except the last `max_stop_len` chars,
            // which we keep in `tail` in case a stop sequence straddles a token boundary.
            if tail.len() > max_stop_len {
                let cut = tail.len() - max_stop_len;
                // Snap to a valid UTF-8 char boundary.
                let cut = (0..=cut).rev().find(|&i| tail.is_char_boundary(i)).unwrap_or(0);
                if cut > 0 {
                    let to_emit = tail[..cut].to_string();
                    tail = tail[cut..].to_string();
                    app_clone
                        .emit("llm-token", LlmToken { text: to_emit, done: false })
                        .ok();
                }
            }

            batch.clear();
            batch.add(new_token, n_pos, &[0], true)
                .map_err(|e| format!("Token batch add failed: {e}"))?;
            ctx.decode(&mut batch).map_err(|e| format!("Token decode failed: {e}"))?;
            n_pos += 1;
        }

        // Flush any buffered tail that didn't reach the emit threshold.
        if !tail.is_empty() {
            let stop_pos = STOP_SEQS.iter().filter_map(|s| tail.find(s)).min();
            let to_emit = match stop_pos {
                Some(pos) => tail[..pos].to_string(),
                None => tail,
            };
            if !to_emit.is_empty() {
                app_clone
                    .emit("llm-token", LlmToken { text: to_emit, done: false })
                    .ok();
            }
        }

        app_clone
            .emit("llm-token", LlmToken { text: String::new(), done: true })
            .ok();

        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task panicked: {e}"))??;

    Ok(())
}
