# AI Buddy — Session State & Next Steps

Last updated: 2026-06-11

---

## What Works

- **Onboarding flow**: welcome → model download → accessibility permission → ready
- **Onboarding accessibility step** (2026-06-16, reworked): auto-skips when already trusted (mount-check → `onNext`); on Continue shows macOS's own AX prompt via `prompt_accessibility_permission` (`AXIsProcessTrustedWithOptions` + `kAXTrustedCheckOptionPrompt`) rather than blindly opening Settings; polls, and after ~5 s of no detection surfaces a "Restart AI Buddy to finish" button (`restart_app` → `app.restart()`) because Accessibility grants only take effect for a freshly-launched process. Restart is safe re-download-wise: `ModelDownload` auto-advances when the model already exists. NOTE: macOS has NO one-click "Allow" dialog for Accessibility (control-your-computer) — the Settings toggle is mandatory; the AX prompt is the closest official ask. `restart_app` does NOT use `app.restart()` (that calls `std::process::exit` → the aborting `__cxa_finalize_ranges` finalizers) — it spawns a fresh instance (`open -n <bundle>` bundled, or the dev binary) then `libc::_exit(0)` to skip finalizers.
- **Model download**: streams from `unsloth/gemma-4-E4B-it-GGUF` (~5 GB), no HuggingFace login required
- **LLM inference**: llama-cpp-2, Metal GPU acceleration, streaming tokens to chat UI
- **Echo dedup (text-level)** (2026-06-18): on speakers, the mic re-hears participants → their words appear as both "Them" and "Me". `dedup_echo` in `transcription.rs` (run at the top of `save_transcript`, before diarization/subject/summary) drops a "me" segment when a nearby "them" segment is textually near-duplicate: temporal proximity (|start_sec| ≤ 2.5 s, or ts_ms within 6 s) AND token similarity (Jaccard ≥ 0.7 OR containment ≥ 0.85), requiring the "me" side to have ≥ 3 tokens (short utterances kept). Only "me" removed; self-gating (headphones → no match → no drops). Unit-tested (`#[cfg(test)] mod tests`). Doesn't help true double-talk — that's the audio-AEC follow-up.
- **Mic AEC: VPIO tried and REVERTED** (2026-06-18): to fix speaker-bleed (mic picking up other participants → labeled "Me"), tried `setVoiceProcessingEnabled:YES` on the mic input node. Result: it drastically ducked the mic — even lowered the **system** input level (visible in Sound settings) — so "Me" stopped transcribing. Reverted to raw mic. Lesson: macOS VPIO over-suppresses for this use and its AEC reference likely isn't other-app output anyway. **Real fix when revisited: software AEC** (WebRTC AEC3 / SpeexDSP) fed mic − the ScreenCaptureKit system audio we already capture as the reference; doesn't touch the system mic. Until then, headphones give clean Me/Them separation.
- **Inline edit preserves line breaks** (2026-06-17): edited text used to lose paragraphs/line breaks because the model delivered it inside a JSON string (`replace_selected_text` tool args) — a 4B model keeps multi-paragraph JSON valid by collapsing it to one line. Fixed by carrying the replacement as RAW text between `<replace>…</replace>` tags (no JSON escaping): `parseEditBlock` in `agent.ts` extracts it verbatim (strips one leading/trailing newline) and routes it through the existing `replace_selected_text` handler; `visibleText` hides the block while streaming. `replace_selected_text` removed from the JSON tool list. Frontend-only.
- **Inline edit paste fallback** (2026-06-17): editing highlighted text in browser/web fields (Gmail in Chrome) previously failed as "read-only" because `replace_selected_text` only did an AX write, which Chrome's web inputs reject even though they're user-editable. `replace_selected_text_impl` (`accessibility.rs`) now tries the AX write first (native apps like Mail), then falls back to **paste-over-selection**: save clipboard → set edited text → `activate_app(pid)` (NSRunningApplication) → `CGEventPostToPid(pid, ⌘V)` → restore clipboard. The agent's `__READ_ONLY__` branch is now a rare last resort (only when there's no target PID).
- **Summarize feature** (2026-06-15, text-only; image/PDF later): (1) Chat — asking the droid to summarize attached/selected/pasted text produces a **streaming** bulleted summary directly (system-prompt guidance in `agent.ts`, no tool round-trip; resource text already reaches the LLM via `resourceContext`). (2) Paste — `onPaste` in ChatPanel captures *substantial* text (>200 chars or >2 lines) as a resource chip and injects "summarize or edit?" (short pastes pass through); ⌥Space over a highlight does the same via the `hotkey-triggered` listener. (3) Meeting minutes — saved `.md` now has `## AI-Generated Summary` (bulleted Key Points/Decisions/Action Items, generated at save via `transcription::generate_summary`) above `## Transcript`; the live "Meeting in progress" file shows a placeholder until Stop. Shared core: `llm::generate_text` (multi-line, n_ctx 4096, input-truncating) + `summarize_text` Tauri command (frontend/future image-PDF entry point). NOTE: both `generate_response` and `generate_text` set `with_n_batch(4096)` — the llama.cpp default n_batch (2048) is smaller than our n_ctx and asserts (`n_tokens_all <= n_batch`) on long prompts (a pasted article as chat context first exposed this).
- **Stop sequences**: rolling buffer catches `<end_of_turn>` and `<start_of_turn>` even when generated as character tokens rather than the single special token
- **SQLite memory**: `store_preference`, `get_all_preferences`, `delete_preference` — all compile and work (tested via devtools console)
- **Agent loop**: TypeScript tool-calling loop wired into ChatPanel, up to 5 tool-call rounds
- **Markdown rendering**: `react-markdown` in buddy bubbles — bold, lists, paragraphs, code blocks all render correctly
- **Copy-paste**: chat bubbles have `user-select: text` so copying works
- **Chat UI**: drag to move, close button, streaming cursor, typing indicator, file drop
- **Transcription engine (2026-06-12): SpeechAnalyzer on macOS 26+, SFSpeechRecognizer fallback below.** Production logs proved SFSpeechRecognizer runs ONE on-device task at a time — starting a lane's task evicted the other lane's within ~40 ms (`kAFAssistantErrorDomain 1110`, perfectly alternating in logs), making concurrent mic+system transcription impossible and causing the persistent text loss at speaker switches. The new engine: `src-tauri/src/speech_analyzer.swift` (compiled by `build.rs` via `xcrun swiftc -emit-library -static`, Swift runtime dylibs from `/usr/lib/swift`), two concurrent SpeechAnalyzer+SpeechTranscriber pipelines (officially supported; same config → shared backing model), volatile→finalized results map straight onto the `is_final` callback — NO gating/rotation/pending-flush needed. Runtime dispatch in `capture.m` (`AiBuddyAudioSink` protocol; `AiBuddyAnalyzerSink` forwards buffers to Swift; legacy `AiBuddySpeechLane` path kept for macOS 13–25, where all the gating/rotation/pending-flush machinery still applies). SpeechAnalyzer needs NO Speech Recognition TCC (auth_status reports "authorized" when available). On-device model via `AssetInventory`: `speech_assets_status` / `install_speech_assets` commands (`speech_assets.rs`, `speech-assets-progress` events); TranscriptPanel installs on first Start if needed (often already present via Dictation).
- **Transcript panel**: real-time speech-to-text via Apple's on-device SFSpeechRecognizer (no model download, automatic punctuation). Two parallel recognizers: microphone (AVAudioEngine) labeled "Me", system audio (ScreenCaptureKit — meeting participants) labeled "Them". Live partial results shown italic, finalized into speaker-labeled, timestamped turns. Copy / →Chat output `Me: … / Them: …` format. Engine choice is Mac-only by design (decided 2026-06-10; a Windows/Linux port would need a different speech backend AND a new audio capture layer anyway)
- **Transcript persistence**: Rust `TranscriptStore` (managed state) is the source of truth — final segments accumulate there; `get_transcript` command restores the panel on remount (tab switches previously destroyed the transcript). Cleared on Start (safe: previous transcript auto-saved on Stop)
- **Unified memory store** (2026-06-11): single SQLite `memory` table — every row is a `rule` (freeform, consumed by the LLM via system prompt) or a `setting` (key-value, consumed by Rust, e.g. `transcript_dir`). One-time migration from the old `preferences`/`settings` tables runs in `init_db` (verified against the real db). One list in the Memory window (`describeMemory` in `src/lib/memory.ts` renders friendly labels); deleting a setting row reverts it to its default. Settings are also injected into the agent's system prompt, so the buddy can answer "where do you save my transcripts?"
- **Transcript save failure handling** (2026-06-11): if the configured dir fails, the save falls back to the default dir; if both fail, `transcript-save-failed` is emitted and chat tells the user the transcript is still copyable from the Transcript tab. The store is never cleared on failure (only on the next Start). Save logs the resolved dir.
- **Transcript auto-save**: on Stop, a background thread waits ~2.5 s for trailing finals, generates a 3–5 word subject via local Gemma (`llm::generate_short_text`, non-streaming; falls back to first words if model busy/unloaded), and writes a speaker-labeled markdown file. Default `~/Documents/AI Buddy Transcripts/YYYY-MM-DD HHMM - Subject.md` (no colons — illegal on macOS); collisions get " (2)". Settings in SQLite `settings` table: `transcript_dir`, `transcript_include_time` — changeable from chat via the agent's `set_transcript_settings` tool ("save minutes to my Desktop and omit the time")
- **Transcription events in chat**: Rust emits `transcription-started` / `transcription-stopped` / `transcript-saved` (path payload); ChatPanel injects buddy messages without an LLM call. The saved-file message links via `aibuddy-reveal://<encoded path>` — a custom ReactMarkdown `a` component intercepts clicks and calls `reveal_in_finder` (`open -R`)

---

## What Is NOT Working

### Highlighted text / accessibility (top priority)

**Symptom**: When the user selects text in another app (e.g. Mail, Chrome) then clicks AI Buddy and asks it to edit the text, the agent reports "no focused text field" and does nothing.

**Root cause**: Two compounding problems:
1. **No accessibility permission on the dev binary** — The raw `target/debug/aibuddy` binary is not in `System Settings → Privacy & Security → Accessibility`. macOS blocks all AX calls without it. The bundled `.app` shows up automatically; the dev binary does not.
2. **Focus shift** — When the user clicks the droid overlay, macOS shifts keyboard focus to the AI Buddy window. `AXFocusedUIElement` then points at AI Buddy's own text input, not the email the user had selected.

**Fix already implemented** (code is written and compiles, not yet verified working):
- On droid mousedown, `save_frontmost_app` is called — it captures the PID of whatever app was active
- All AX read/write calls now use `AXUIElementCreateApplication(saved_pid)` to reach into that app even after it loses focus
- Files changed: `src-tauri/src/accessibility.rs`, `src-tauri/src/lib.rs`, `src/components/Droid/DroidOverlay.tsx`

**What still needs to happen**:
1. Grant the dev binary accessibility permission (see instructions below)
2. Test the end-to-end flow: select text in Mail → click droid → ask AI Buddy to edit → verify text gets replaced
3. Debug if it still doesn't work (check if `save_frontmost_app` is firing at the right moment)

**How to grant dev binary AX permission**:
```
# Open the Accessibility pane
open "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
# Click + and navigate to:
# src-tauri/target/debug/aibuddy
```

---

## Unfinished Features

### Per-platform transcription backends (planned, not started)
Today transcription is macOS logic inline-`#[cfg]`-gated in `transcription.rs` (the `extern "C"` FFI, `on_speech`, and the macOS branches of start/stop/auth); non-mac returns "only supported on macOS". Compiles everywhere but there's no clean place a Windows/Linux impl would slot in.
Planned structure (Mac stays as-is, just moved): a compile-time-selected `backend` module —
```
#[cfg_attr(target_os = "macos",   path = "transcription/macos.rs")]
#[cfg_attr(target_os = "windows", path = "transcription/windows.rs")]
#[cfg_attr(not(any(...)),         path = "transcription/linux.rs")]
mod backend;
```
with a neutral surface (`available / auth_status / request_auth / start(record_path) / stop / assets_*`). Platform-agnostic core (TranscriptStore, save_transcript, render_markdown, generate_subject/summary, live file, and a new `deliver_segment(source,text,is_final,start,end)` seam the backends call) stays in `transcription.rs`; the `#[tauri::command]`s lose their inline `#[cfg]`. `build.rs` gains windows/linux arms (stubbed). Follow `accessibility.rs`'s `mod mac` precedent. Diarization (`sherpa-rs`) is already cross-platform — only the audio recording feeding it is Mac-specific. Real Windows (WASAPI loopback + STT) / Linux (PipeWire monitor + STT) capture engines are separate future work.

### Image input via Gemma 4 multimodal (not started)
- Gemma 4 is multimodal but our llama-cpp-2 integration is text-only
- Requires:
  1. Download the `.mmproj` (multimodal projection) file alongside the main GGUF
  2. Load the clip/vision model in Rust at startup
  3. Encode dropped images via llama-cpp's LLaVA API into embeddings
  4. Pass encoded image embeddings into the context before text tokens
  5. Frontend: send image as base64 or raw bytes from the resource chip to Rust
- Reference: llama-cpp LLaVA C API (`llava_image_embed_make_with_bytes`, `llava_eval_image_embed`)


### Detail Panel (not started)
- Spec calls for a side panel with: Memory tab (view/edit/delete rules), History, About/settings
- The `[⊞]` button in ChatPanel.tsx exists but does nothing
- New component needed: `src/components/DetailPanel/`

### Versioning (2026-06-17)
- `npm run release` (patch), `release:minor`, `release:major` → `scripts/bump-version.mjs` bumps the version in `tauri.conf.json` (canonical bundle version), `package.json`, and `Cargo.toml` in sync, THEN runs `tauri build`.
- `npm run bump [patch|minor|major]` bumps without building.
- Deliberately a pre-build script, NOT a `beforeBuildCommand` hook: Tauri reads `tauri.conf.json`'s version when the build starts, so an in-build hook would only affect the *next* build.
- `npm run tauri build` directly still works but does NOT bump (use it for test builds).

### Packaging / Distribution (working as of 2026-06-11)
- `npm run tauri build` → `.dmg` at `src-tauri/target/release/bundle/dmg/` (~3.7 MB; Gemma model downloads on first launch, not bundled)
- `minimumSystemVersion: "13.0"` in tauri.conf.json is REQUIRED — without it tauri sets MACOSX_DEPLOYMENT_TARGET=10.13 and llama.cpp's `std::filesystem` (10.15+) fails to compile. Gotcha: CMake caches the deployment target — if the error persists after fixing the conf, `rm -rf src-tauri/target/release/build/llama-cpp-sys-2-*`
- Bundled Info.plist verified: mic + speech usage strings present, LSMinimumSystemVersion 13.0
- Unsigned: recipients must right-click → Open (or `xattr -cr`) to bypass Gatekeeper. Apple Developer ID + notarization needed for real distribution
- aarch64-only; Intel/universal needs `rustup target add x86_64-apple-darwin` + `--target universal-apple-darwin`

### `read_file` tool (stubbed)
- `agent.ts` returns `"(file reading not yet implemented)"` — file drop UI works but reading content does not

---

## Known Issues / Quirks

- **Model size label**: UI says ~2.7 GB but the unsloth model is actually ~5 GB. `TOTAL_GB` in `src/onboarding/ModelDownload.tsx` is set to `5.0` now.
- **`<end_of_turn>` still possible via ReactMarkdown**: If the rolling buffer misses something and `<end_of_turn>` ends up in the buffer, it renders as plain text in the UI. Could add a final `.replace(/<end_of_turn>|<start_of_turn>/g, '')` in `agent.ts` line 167 as a safety net.
- **⚠️ Never add a second ggml-based crate**: whisper-rs (since removed) and llama-cpp-2 each statically link their own bundled ggml with identical C symbol names. The linker keeps one copy, llama.cpp ran against whisper's older ggml, and the Gemma model failed to load ("tensor 'per_layer_model_proj.weight' is duplicated") with Metal lost. Fixed 2026-06-10 by removing whisper-rs. Any future ggml-linking crate (whisper-rs, other llama bindings, stable-diffusion.cpp bindings, etc.) must run in a separate process.
- **Transcript save: not throttled in background + correct diarization** (2026-06-18): (1) The post-Stop save (diarization + 2 LLM passes) ran on a plain background thread that macOS App Nap throttled when the app wasn't frontmost — work crawled (~an hour, finishing only on refocus). Fixed with an NSProcessInfo activity assertion: `aibuddy_begin/end_processing_activity` (`capture.m`, `NSActivityUserInitiated`) wrapped by an RAII `ActivityGuard` at the top of `save_transcript`. (2) Diarization fed the WAV at its recorded rate (SpeechAnalyzer's ~48 kHz) to 16 kHz-trained models → slow + 57 phantom speakers; now `diarization::diarize` resamples to 16 kHz (linear `resample_to_16k`, no new dep) and uses clustering `threshold 0.7` (was 0.5). (3) Safety net: `apply_diarization` keeps "Them" instead of garbage labels when diarization returns > 10 distinct speakers (`MAX_TRUSTED_SPEAKERS`).
- **Speaker diarization** (2026-06-14): the "Them" stream is split into Speaker 1/2/3 in the saved file, OpenWhispr-style. Local, post-meeting: during a session the SpeechAnalyzer engine tees the converted Them audio to `<app data>/them-session.wav` (Swift `AVAudioFile` in `speech_analyzer.swift`, only when models installed); on Stop, `save_transcript` runs `diarization::diarize` (sherpa-onnx: pyannote-segmentation-3.0 + 3D-Speaker CAM++ via `sherpa-rs` crate, `download-binaries` feature — prebuilt onnxruntime/sherpa dylibs, NO ggml so no llama collision), then `apply_diarization` relabels each "them" segment by max time-overlap (segments carry audio-relative `start_sec`/`end_sec` from SpeechTranscriber's `.audioTimeRange`). WAV always deleted after. Models (~36 MB) download on demand via `diarization_models_status`/`install_diarization_models` (segmentation is a tar.bz2 extracted with system `tar`); TranscriptPanel shows a download note when missing. Mic ("Me") is never diarized. Pre-26 legacy engine has no time ranges → no recording → no diarization (stays Me/Them).
- **Quit-time SIGABRT after transcription** (2026-06-14, FIXED): the bundled release app aborted on quit *after* a transcription session (crash stack: `NSApplication terminate:` → `exit()` → `__cxa_finalize_ranges` → `abort`, all `aibuddy` frames). Cause: the statically-linked Swift SpeechAnalyzer engine's process-global `SAEngine.lanes` holds `SpeechAnalyzer` actors + Tasks; tearing those Swift concurrency objects down inside `cxa_finalize` aborts. Fresh launch+quit was clean (lanes empty) — confirming the trigger is having started transcription. Fix: `lib.rs` now uses `.build()?.run(|_app, event| …)` and calls `libc::_exit(0)` on `RunEvent::Exit`, bypassing the aborting finalizers. Safe: SQLite autocommits, transcripts write incrementally (live file has all finals). Minor: quitting within ~4.5 s of Stop skips the subject-rename (file keeps "Meeting in progress" name but full content).
- **Distributable dylib bundling** (2026-06-14, DONE): the bundled `.app` now ships `libsherpa-onnx-c-api.dylib` + `libonnxruntime.1.17.1.dylib` in `Contents/Frameworks/` via `bundle.macOS.frameworks` in tauri.conf.json, and `build.rs` bakes an `@executable_path/../Frameworks` rpath into the binary (dev worked only via cargo's injected dylib path — a Finder-launched .app had none). Verified: bundled binary's rpath + both dylibs present + dyld maps them into the live process. ⚠️ The onnxruntime filename version (`1.17.1`) is hardcoded in tauri.conf.json — if sherpa-rs bumps onnxruntime, update that path or `tauri build` fails at bundling.
- **Save-progress UX** (2026-06-18): the post-Stop processing (tens of seconds) used to show only a generic "finalizing…". `save_transcript` now emits `transcript-progress` events ("Identifying speakers…", "Writing summary…"); the TranscriptPanel status bar shows the live stage with a spinner, and ChatPanel injects one heads-up line ("Writing up your meeting notes… 📝", deduped per session via `processingNotedRef`) so users on the Chat tab also get feedback between "stopped" and "stored".
- **Transcript status bar** (2026-06-12): between transcript area and buttons. Keyed on live-file existence, not the transcribing flag: recording → pulsing dot + live-file link + "saved"/"unsaved changes" (unsaved = partials exist; the live file only gets finals); Stop→rename window → link + "finalizing…" (or "save failed — file kept", with `transcript-save-failed` keeping the link for recovery); after save → "Saved to <file>" link. Empty session → backend emits `transcript-discarded` (live file deleted) and the bar clears. Backend: `TranscriptStore.last_saved` + `get_transcript_files` (survives tab-switch remounts).
- **Live meeting-notes file** (2026-06-11, replaced the recovery journal): `YYYY-MM-DD HHMM - Meeting in progress.md` is created in the transcript folder at session start (chat's started message links to it) and fully re-rendered on every final segment, so the user can watch it grow. On save: subject generated → file rewritten with real header → renamed to `… - Subject.md` (include-time setting honored, collisions get " (2)"). Empty session → live file deleted. Crash mid-meeting → "Meeting in progress" file remains with everything up to the last final. Live updates are finals-only (partials are volatile).
- **Transcript meeting-resilience** (2026-06-11, after live-meeting failures): lanes NEVER kill the session on errors anymore — repeated errors (>5 in 10 s) put the lane in a 30 s cooldown then the energy gate retries (`_errorCooldownUntil` in capture.m); a chat warning ("Heads up: …") is emitted via `transcription-warning` (source = -2). Fatal `transcription-error` (-1) now also triggers a save so an abnormal stop can't lose the file. Natural mid-stream finals can be TRUNCATED vs the last partial — the handler delivers whichever is longer (the dropped tail's audio is gone from recognition, so this can't duplicate).
- **Transcript: speech permission in dev**: TCC entries for the unbundled dev binary can invalidate across rebuilds (cdhash changes), so re-prompting for Speech Recognition in dev is normal. Stale denial: `tccutil reset SpeechRecognition`.
- **Transcript: request rotation**: recognition requests are rotated every 50 s mid-monologue (Apple guidance ~1 min/request). If text ever drops at a rotation seam, look at `_rotate` in `capture.m`.
- **Transcript: pending-flush at request ends** (2026-06-11, v2 — replaced immediate flush + suppression, which lost 10–20 s chunks at every rotation because partials lag the audio and the suppressed final often had MORE text). When a request ends (gate close, 25 s rotation, Stop), `_endRequestPending` in `capture.m` snapshots the last partial and waits for the ended task's final; resolution = longer(final, partial), triggered by the final, an error, a 2 s timeout, or the first result of the next request (ordering guarantee — finals never land out of order). Watch `pending resolved by <reason> (+N chars vs partial)` logs to see tail recovery. Lanes outlive stop() by 4 s; the save task waits 4.5 s.
- **Transcript: energy-gated recognition**: lanes only run a recognition task while there is sound on them (RMS gate 0.008, 2 s hold in `capture.m`). Continuously-running tasks on silent lanes churned `kAFAssistantErrorDomain 1110` errors every ~350 ms and destabilized BOTH lanes' recognition (fixed 2026-06-10). Benign errors (1110/203/216/301) send the lane idle; the gate reopens on sound. Trade-off: ~100–200 ms of audio at utterance onset is lost while the gate opens.
- **Orphaned whisper model file**: `~/Library/Application Support/com.aibuddy.app/models/ggml-base.en.bin` (145 MB) is no longer used and can be deleted.
- **Droid drag conflict**: `onMouseDown` in DroidOverlay calls both `startDragging()` and `save_frontmost_app()` — these race. If the user is dragging (not clicking), `save_frontmost_app` fires unnecessarily but harmlessly.
- **`onboarding_complete` flag**: If this file exists but the model isn't downloaded, the app silently skips download. Delete `~/Library/Application Support/com.aibuddy.app/onboarding_complete` to re-run onboarding.

---

## How to Run

```bash
# Install deps (first time only)
brew install cmake
cd /Users/almithani/projects/smbsoft/aibuddy
npm install

# Run dev server
npm run tauri dev
```

Model is stored at:
```
~/Library/Application Support/com.aibuddy.app/models/gemma-4-E4B-it-Q4_K_M.gguf
```

---

## Key Files

| File | Purpose |
|------|---------|
| `src-tauri/src/llm.rs` | LLM inference, streaming, stop-sequence rolling buffer |
| `src-tauri/src/accessibility.rs` | macOS AX layer — get/set selected/focused text, `PrevApp` state |
| `src-tauri/src/download.rs` | Model download, `model_path()` checks resource dir then app data dir |
| `src-tauri/src/memory.rs` | Unified SQLite `memory` table (rules + settings), legacy-table migration |
| `src-tauri/src/lib.rs` | Tauri setup, window management, command registration |
| `src/lib/agent.ts` | TypeScript tool-calling agent loop |
| `src/components/ChatPanel/ChatPanel.tsx` | Chat UI, streams tokens, calls `runAgent` |
| `src/components/Droid/DroidOverlay.tsx` | Droid click/drag, calls `save_frontmost_app` on mousedown |
| `src/onboarding/ModelDownload.tsx` | Download progress UI |
| `src-tauri/.cargo/config.toml` | macOS 26 SDK C++ header fix (required for llama-cpp-2 to build) |
| `src-tauri/tauri.conf.json` | Three-window config (onboarding, droid, chat) |
| `src-tauri/src/transcription.rs` | Thin Rust layer: speech permission commands, start/stop, segment/error event emission |
| `src-tauri/src/capture.m` | ObjC transcription engine: ScreenCaptureKit (system audio) + AVAudioEngine (mic), each feeding an SFSpeechRecognizer lane with request rotation and error recovery |
| `src-tauri/Info.plist` | Usage descriptions (speech recognition, microphone) — embedded in dev binary AND merged into bundled .app |
| `src/components/TranscriptPanel/TranscriptPanel.tsx` | Transcript UI: permission flow, start/stop, speaker-turn display with live partials, send-to-chat |

---

## Tomorrow's Priority Order

1. **Verify accessibility permission** — add dev binary to AX list, re-test inline editing
2. **Debug `save_frontmost_app` timing** — if AX still fails, add a `console.log` in devtools to confirm the PID being saved is the external app's PID (not `0` or AI Buddy's own PID)
3. **Test full inline editing loop** — select text → click droid → "clean this up" → text replaced
4. **If AX works**: build the Detail Panel (memory management UI)
5. **Then**: `npm run tauri build` for the `.dmg`
