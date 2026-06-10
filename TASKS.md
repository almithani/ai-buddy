# AI Buddy — Session State & Next Steps

Last updated: 2026-06-10

---

## What Works

- **Onboarding flow**: welcome → model download → accessibility permission → ready
- **Model download**: streams from `unsloth/gemma-4-E4B-it-GGUF` (~5 GB), no HuggingFace login required
- **LLM inference**: llama-cpp-2, Metal GPU acceleration, streaming tokens to chat UI
- **Stop sequences**: rolling buffer catches `<end_of_turn>` and `<start_of_turn>` even when generated as character tokens rather than the single special token
- **SQLite memory**: `store_preference`, `get_all_preferences`, `delete_preference` — all compile and work (tested via devtools console)
- **Agent loop**: TypeScript tool-calling loop wired into ChatPanel, up to 5 tool-call rounds
- **Markdown rendering**: `react-markdown` in buddy bubbles — bold, lists, paragraphs, code blocks all render correctly
- **Copy-paste**: chat bubbles have `user-select: text` so copying works
- **Chat UI**: drag to move, close button, streaming cursor, typing indicator, file drop
- **Transcript panel**: real-time speech-to-text via Apple's on-device SFSpeechRecognizer (no model download, automatic punctuation). Two parallel recognizers: microphone (AVAudioEngine) labeled "Me", system audio (ScreenCaptureKit — meeting participants) labeled "Them". Live partial results shown italic, finalized into speaker-labeled, timestamped turns. Copy / →Chat output `Me: … / Them: …` format. Engine choice is Mac-only by design (decided 2026-06-10; a Windows/Linux port would need a different speech backend AND a new audio capture layer anyway)

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

### Packaging / Distribution (not started)
- `npm run tauri build` → `.dmg` installer
- Required before sharing with anyone
- Also required for proper accessibility permission flow in production (bundled `.app` auto-registers)
- Note: `src-tauri/tauri.conf.json` has `"resources": []` — if bundling the model, change to `{"resources/models/*": "models/"}` and copy the GGUF to `src-tauri/resources/models/` first

### `read_file` tool (stubbed)
- `agent.ts` returns `"(file reading not yet implemented)"` — file drop UI works but reading content does not

---

## Known Issues / Quirks

- **Model size label**: UI says ~2.7 GB but the unsloth model is actually ~5 GB. `TOTAL_GB` in `src/onboarding/ModelDownload.tsx` is set to `5.0` now.
- **`<end_of_turn>` still possible via ReactMarkdown**: If the rolling buffer misses something and `<end_of_turn>` ends up in the buffer, it renders as plain text in the UI. Could add a final `.replace(/<end_of_turn>|<start_of_turn>/g, '')` in `agent.ts` line 167 as a safety net.
- **⚠️ Never add a second ggml-based crate**: whisper-rs (since removed) and llama-cpp-2 each statically link their own bundled ggml with identical C symbol names. The linker keeps one copy, llama.cpp ran against whisper's older ggml, and the Gemma model failed to load ("tensor 'per_layer_model_proj.weight' is duplicated") with Metal lost. Fixed 2026-06-10 by removing whisper-rs. Any future ggml-linking crate (whisper-rs, other llama bindings, stable-diffusion.cpp bindings, etc.) must run in a separate process.
- **Transcript: speech permission in dev**: TCC entries for the unbundled dev binary can invalidate across rebuilds (cdhash changes), so re-prompting for Speech Recognition in dev is normal. Stale denial: `tccutil reset SpeechRecognition`.
- **Transcript: request rotation**: recognition requests are rotated every 50 s mid-monologue (Apple guidance ~1 min/request). If text ever drops at a rotation seam, look at `_rotate` in `capture.m`.
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
| `src-tauri/src/memory.rs` | SQLite preferences |
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
