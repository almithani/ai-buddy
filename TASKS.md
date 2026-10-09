# AI Buddy — Session State & Next Steps

Last updated: 2026-10-09

Legend: ✅ confirmed by the user in the app · ⏳ built and checked by tests/harnesses, not yet confirmed live

---

## Release plan (decided 2026-10-09)

- **0.2 — alpha, Apple Silicon only, installed personally.** No Apple Developer ID / notarization: builds are ad-hoc signed (`signingIdentity: "-"`, `hardenedRuntime: false`), so each new build needs the Accessibility permission re-granted (remove + re-add) and a right-click → Open on first launch. Build: `npm run bump minor` (→ 0.2.0), then `npm run tauri build` (Apple Silicon) — or `npm run build:universal` if a universal dmg is wanted anyway. Commit + `git tag v0.2.0`.
  - [x] Private-content logs removed (2026-10-09).
  - [x] Mic permission text (`NSMicrophoneUsageDescription`, `src-tauri/Info.plist`) mentions holding Option-Space as well as meetings.
  - [x] Keyboard focus after ⌥Space goes to the chat ✅ (real keyboard, 2026-10-09).
  - [x] TASKS.md cleaned up (2026-10-09).
- **0.3 — Intel support (universal build).** Confirm the CPU-only fix on the real Intel Mac; AVX2/FMA/F16C for the x86_64 llama.cpp build (speed); check hold-to-talk on Intel.
- **Later — public release:** Apple Developer ID signing + notarization — set `hardenedRuntime` back to `true`, add the mic entitlement (`com.apple.security.device.audio-input`); permissions then survive updates and no right-click is needed.

---

## Open items

**Not yet confirmed live** (⏳ — see the matching TESTS.md checklists):
- Chat follows macOS display settings (text size, contrast, transparency, motion, light/dark). Also unconfirmed: which preference key the macOS 26 Text Size slider writes (both are read).
- Everyday how-to guides (47 topics).
- Instant personalised greeting.
- Shorter replies (`LENGTH_RULES`).
- Buddy knows its own features (`FEATURES`).
- No system "ding" on ⌥Space in native apps.
- Settings navigator extras: "already on" short-circuit, sub-page relaunch, `get_mac_info`.

**Known gaps worth fixing soon:**
- **Long chats eventually fail with "Input is too long…"** — history is never trimmed (see "Shrink the system prompt" below; step 2 alone fixes it).
- **Read-aloud guard:** `speak_replies` was found set to `always` without the user remembering asking — only allow `set_voice_settings` when the message is actually about reading aloud / voice.
- **Onboarding screens are dark-only** (`onboarding.css` and `AccessibilityPermission.tsx` still have dark-tuned literal colors; not converted to the theme tokens).
- **Settings step wording written from memory, not probed** — spot-check: Night Shift "Schedule", Displays "Larger Text", Trackpad "Scroll & Zoom" → "Natural scrolling", Control Center → Battery "Show Percentage".
- **Hold-to-talk edge:** pressing ⌥Space again within ~0.3 s of releasing can reset the shared `Heard` before the previous result is sent.
- Not verified: speech recognition quality on Bluetooth in a real meeting; dictation + meeting mic open together on Bluetooth (user reports holding ⌥Space during a transcription "works well").

---

## What Works

### Chat, agent and model
- **Onboarding** (welcome → model download → accessibility → ready). The accessibility step shows macOS's own prompt (`prompt_accessibility_permission`), polls, and after ~5 s offers "Restart AI Buddy". `restart_app` spawns a fresh instance (`open -n <bundle>`, or the dev binary) then `libc::_exit(0)` — NOT `app.restart()`, whose `exit()` runs the aborting `__cxa_finalize_ranges` finalizers. macOS has no one-click "Allow" for Accessibility; the AX prompt is the closest official ask.
- **Model download**: `unsloth/gemma-4-E4B-it-GGUF` (~5 GB), no HuggingFace login. Stored at `~/Library/Application Support/com.aibuddy.app/models/gemma-4-E4B-it-Q4_K_M.gguf`.
- **LLM inference** (llama-cpp-2): Metal GPU on Apple Silicon, **CPU only on Intel** (`USE_GPU` in `llm.rs`); streaming tokens to the chat. Stop sequences caught by a rolling buffer even when `<end_of_turn>` arrives as character tokens. `generate_response` / `generate_text` set `with_n_batch(4096)` (llama.cpp's default 2048 < n_ctx asserts on long prompts).
- **Agent loop** (`agent.ts`): JSON tool calls, up to 5 rounds; tools: `read_file`, `store_preference`, `get_memory`, `set_transcript_settings`, `open_settings`, `show_howto`, `set_voice_settings`, `get_mac_info`; in-place edits via raw `<replace>…</replace>` blocks.
- **System prompt** (`buildSystemPrompt`): identity → `FEATURES` (what the buddy can do — keep in sync, see CLAUDE.md) ⏳ → `TOOL_DOCS` + rules → memory rules/settings → top 6 settings topics + top 4 how-tos for this message → `LENGTH_RULES` last (≤3 sentences/steps, no preamble/closing line; summaries ≤5 bullets) ⏳. ≈1,600 tokens of the 4,096 context (estimate).
- **Chat UI**: markdown bubbles (copyable), drag to move, file drop, single waiting indicator (typing dots — no empty bubble/cursor; also covers tool rounds), instant greeting.
- **Instant personalised greeting** ⏳ (`greeting.rs`): generated once per rules change (`store_preference` / `delete_memory`, plus a catch-up after model load) and cached as hidden setting `_greeting` = `{rulesHash, text}`; shown only if the hash matches, else "What can I help you with?". Output validated (≤160 chars, one line, no `<`).
- **Unified memory** (`memory.rs`): one SQLite `memory` table of `rule`s (go into the prompt) and `setting`s (read by Rust, e.g. `transcript_dir`, `speak_replies`); Memory panel (≡ button, `DetailPanel.tsx`) lists/deletes them with friendly labels (`describeMemory`). `_`-prefixed keys are internal caches, hidden from the panel and the prompt.
- **`read_file`**: dropped text files are read into the conversation (images/PDFs are refused with a note; files truncated to 2,500 chars).
- **Accessibility permission UX** ✅: when missing, the chat shows macOS's prompt plus clickable steps (`aibuddy-action://open-accessibility` → `com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility`, `aibuddy-action://restart`), and tells users to remove/re-add a stale entry. With a properly signed build the 2 s poll picks up the grant without a restart.

### Selected text, inline edit, summarize
- **⌥Space** ✅: the chat appears immediately (`show_chat_without_focus` → `orderFrontRegardless`), the selection is captured on a background thread, then the chat takes focus without dropping behind the other app (`focus_chat_keep_front`), switches to the Chat tab, and `hotkey-triggered` fires. Capture = `AXSelectedText` from the saved front-app PID; if empty in a native text control (`AXTextArea`/`AXTextField`) it's trusted (no ⌘C → no system ding ⏳); otherwise ⌘C clipboard fallback (web/Electron). Works in Terminal ✅ and Chrome ✅; not inside Claude Code (its TUI redraw clears the selection — accepted). A missed key-up can't disable later presses (physical Space check, `CGEventSourceKeyState`).
- **Inline edit**: AX write first (native apps), then paste-over-selection fallback (save clipboard → set → activate app → `CGEventPostToPid(⌘V)` → restore) for web fields like Gmail.
- **Summarize**: substantial pastes (>200 chars / >2 lines) and ⌥Space over a highlight become a resource chip + "summarize or edit?"; summaries stream as bullets.

### Hold-to-talk voice (⌥Space held) ✅
- Tap = chat as before (audio discarded). Hold ≥300 ms → grey "Getting the microphone ready… keep holding" card while the mic comes up (Bluetooth headsets take 1.3–1.9 s), then a red "Listening…" card + "pop" sound + red dot on the Chat tab; live words shown; release sends immediately. Letting go too early → "Keep holding ⌥Space until you hear the pop, then talk."
- Own SpeechAnalyzer dictation lane (source 2, own `AVAudioEngine`) — works during meeting transcription ✅. Mic starts on key-down when permission is already granted (prewarmed spare lane), so the first words aren't lost; words kept per time range so a pause can't drop the first phrase; finish waits up to 4 s.
- Bluetooth ✅: the tap uses the device's current format (`format: nil`) and rewires + restarts on `AVAudioEngineConfigurationChange` (headset call-mode switch); session registered before `engine.start()`.
- Replies read aloud (`voice.m`, `AVSpeechSynthesizer`, system voice) when the question was spoken; `speak_replies` = voice (default) | always | never via `set_voice_settings` / Memory panel. Speech stops on the next ⌥Space or send.
- macOS 26+ only (SpeechAnalyzer); older systems get "Talking to me needs macOS 26 or newer."
- Diagnostics: `[voice] mic started …`, `dictation: mic started — <format>`, `audio device changed — restarted with …`, per-session `N buffers, peak level …`, counts-only `[voice] final … N chars` / `sending N chars`.

### Settings navigator ✅ (97 topics)
- User asks → buddy opens the exact System Settings page (deep link from a curated catalog; the LLM only picks a topic id), reads the control's current state, and draws a pulsing orange ring around it (`highlight` window: transparent, click-through, follows scrolling, hides after 20 s / on app switch / when the control vanishes). Repeat requests reopen the page every time ✅.
- Catalog source of truth: `scripts/build-settings-catalog.py` → `src/lib/settingsCatalog.json` (shared by TS + Rust via `include_str!`; never hand-edit). Topics with no single ringable control (Dark Mode, Night Shift, privacy app lists, Storage, Time Machine) open the page with steps only.
- Controls found by `AXIdentifier` (often equals the anchor name — language-independent) or by row label (English; the label is the `AXStaticText` on the same row). Label normalization handles "Wi‑Fi" (U+2011) and "Touch ID" (U+00A0).
- Deep links don't navigate away from some sub-pages (e.g. Text Size) → `open_and_ring` verifies the control appears and relaunches System Settings once if it doesn't.
- `AX_feature.*` anchors are the Accessibility **Shortcut** list, not feature pages (their checkboxes read 1 for everything) — VoiceOver uses `AX_VOICEOVER_ENABLED`.
- Retrieval (`topicSearch.ts`): stemmed token overlap, keywords ×2, phrase bonus, synonym map; right topic in the model's top 6 for 30/30 test phrasings.
- Guards in `agent.ts`: call `open_settings` every time; a no-tool reply that claims settings actions (`SETTINGS_CLAIM`) gets one nudge round; for settings-like messages round 0 isn't streamed (no flash-then-delete).
- `get_mac_info` ⏳: macOS version, battery, Wi-Fi power, volume/mute (fixed read-only commands).
- Verify after catalog edits or a macOS update: TESTS.md §2 (`--check`, `--pages` + probe + `--verify-controls`). Last full verification 2026-09-23: all 97 panes/anchors resolve, all 73 controls found.

### Everyday how-to guides ⏳ (47 topics)
- `scripts/build-howto-catalog.py` → `src/lib/howtoCatalog.json`; `show_howto` returns steps + spoken-friendly shortcut ("Shift-Command-5"), opens the related app only from catalog bundle ids (`howto.rs`, `open -b`; friendly error if e.g. Zoom isn't installed), cross-links settings topics. macOS 26: "open an app" uses the Apps app (Launchpad is gone). Check: `--check-howtos`.

### Easy reading — follows macOS display settings ⏳
- `display_prefs.{m,rs}`: text size from the content-size category (`com.apple.universalaccess` `FontSizeCategory.global` unless `DEFAULT`, else global `UIPreferredContentSizeCategoryName`; scale = body pt / 17, capped 2.0) via 1.5 s poll; contrast / transparency / motion via `NSWorkspace.accessibilityDisplayShould…` + change notification. Emits `display-prefs-changed` on change; resizes the chat to 360×520×scale.
- Frontend (`displayPrefs.ts`): `data-contrast` / `data-transparency` / `data-motion` on `<html>`; `--ui-scale` CSS `zoom` on `#root` (chat only) with `calc(100vw / var(--ui-scale))` sizing (measured: plain `100vw` overflows 1.5× at zoom 1.5). Light/dark via `prefers-color-scheme` tokens in `app.css` (incl. `--hot-mic*`).

### Meeting transcription ✅
- **Engine**: SpeechAnalyzer on macOS 26+ (`speech_analyzer.swift`, built by `build.rs` with `swiftc -emit-library -static`), two concurrent lanes — mic ("Me", AVAudioEngine) and system audio ("Them", ScreenCaptureKit); legacy SFSpeechRecognizer lanes for 13–25 (`capture.m`; SFSpeechRecognizer runs only one on-device task at a time, hence the gating/rotation/pending-flush machinery there). On-device model via `AssetInventory` (`speech_assets.rs`).
- **Panel + persistence**: live speaker-labeled turns; Rust `TranscriptStore` survives tab switches; cleared on Start. Transcript tab shows a red dot while recording ✅.
- **Live notes file**: `YYYY-MM-DD HHMM - Meeting in progress.md` re-rendered on every final; on Stop → subject (local Gemma) → summary (map-reduce for long meetings) → speaker diarization → renamed `… - Subject.md` in `~/Documents/AI Buddy Transcripts` (configurable via chat: `transcript_dir`, `transcript_include_time`). Save falls back to the default folder; failures are reported in chat; empty sessions are discarded.
- **Diarization**: sherpa-onnx (pyannote segmentation + 3D-Speaker CAM++) on the recorded "Them" WAV (resampled to 16 kHz, threshold 0.7, >24 speakers → keep "Them" and say so); models (~36 MB) download on demand. Pre-26 engine: no diarization.
- **Echo dedup**: on speakers, "Me" segments that near-duplicate a nearby "Them" segment are dropped (`dedup_echo`; token Jaccard / containment / char-level similarity; 9 unit tests).
- **Resilience**: lanes never kill the session on errors (cooldown + retry, chat warning); fatal errors still save; App Nap prevented during save (`ActivityGuard`); progress stages shown in the Transcript tab and once in chat; mic path rebuilds the engine on device changes (verified on Bluetooth).
- Chat events: started (with live-file link) / stopped / saved (`aibuddy-reveal://` link → Finder) / warnings.

### Build, packaging, versioning
- `npm run tauri build` → Apple Silicon `.dmg` ✅. `npm run build:universal` → `AI Buddy_<ver>_universal.dmg` (`src-tauri/tauri.universal.conf.json` takes the already-universal sherpa/onnxruntime dylibs from `target/aarch64-apple-darwin/release/`); `npm run release:universal` = patch bump + universal build. Needs `rustup target add x86_64-apple-darwin` (installed).
- Ad-hoc signing of the whole bundle as `com.aibuddy.app` (`signingIdentity: "-"`), hardened runtime off. Verified: `codesign --verify --deep --strict` passes; launches with no dyld errors.
- Bundled dylibs in `Contents/Frameworks/` (+ `@executable_path/../Frameworks` rpath from `build.rs`); min macOS 13.0 for both slices (`tauri build` applies `minimumSystemVersion` even though `.cargo/config.toml` sets 15.0 for dev builds).
- Versioning: `scripts/bump-version.mjs` keeps `tauri.conf.json` (canonical), `package.json` and `Cargo.toml` in sync. `npm run bump [patch|minor|major]` (no build), `npm run release[:minor|:major]` (bump + Apple Silicon build). Pre-build on purpose: Tauri reads the version when the build starts.

---

## Backlog / unfinished features

### Shrink the system prompt / make room for user input (not started, logged 2026-10-06)
**Why:** chat context is 4096 tokens (`generate_response`). System prompt ≈1,600 tokens (estimate) + 512 reserved for the reply leaves ≈2,000 for conversation + pasted text. The **entire chat history is sent every turn and never trimmed**; when prompt + history + 512 > 4096, `generate_response` returns "Input is too long…".

**Ideas, roughly cheapest first:**
1. **Measure first:** log real token counts of system prompt / history / resource per request (`str_to_token` in Rust).
2. **Trim history to a token budget:** keep the most recent turns that fit; drop (or later summarize) older ones. Fixes the hard failure on its own.
3. **Conditional sections:** settings rules only when settings topics matched; how-to rules only when guides matched; `<replace>` editing rules only when text is attached; transcript-folder tool only when transcripts are mentioned.
4. **Fewer retrieved topics:** 6 + 4 → e.g. 4 + 3, or only above a score threshold.
5. **Tighten wording:** shorter tool descriptions, merge overlapping rules, drop anything `LENGTH_RULES` covers.
6. **`FEATURES` on demand:** a `get_features` tool, or include it only when the message asks about the buddy itself (~275 tokens).
7. **Bigger context:** try n_ctx 8192; measure memory + first-token latency (KV cache grows linearly).
8. **Speed, not size:** reuse llama.cpp's KV cache for the unchanged prompt prefix between turns.
- Watch-outs: keep `LENGTH_RULES` last; re-run the settings/how-to/length checks in TESTS.md after any change. Optional debug aid (not built): print the assembled system prompt to the webview console.

### Support older macOS versions (13–25) properly (not started, logged 2026-10-05)
Everything settings-related was built and verified on **macOS 26.6 only**; nothing checks the macOS version at runtime.

**Behaviour on older versions today (degrades, doesn't crash):** settings page opens but anchors may be missing (lands at the top); no ring/state where control ids/labels differ; step wording follows 26 labels; how-tos using the Apps app (26) or Passwords (15+) say "isn't installed"; hold-to-talk says "needs macOS 26"; chat doesn't follow text size (contrast/transparency/motion/light-dark still work).

**Plan:**
1. Decide target versions (likely 14 and 15; 13 if seniors on old Macs matter).
2. macOS VMs per version (UTM on Apple Silicon — free).
3. In each, run TESTS.md §2 (`--check`, probe + `--verify-controls`); save results.
4. Per-version overrides in `scripts/build-settings-catalog.py` (optional `versions: {"15": {anchor, control_id, control_label, steps}}`, falling back to 26), same idea for how-to `app`s (Launchpad / Keychain Access).
5. Runtime: read the major macOS version once; `open_system_settings` / `open_howto_app` / steps pick the override. Keep "LLM only picks a topic id".
6. Extend `--check` / `--verify-controls` to validate against a given version's dump.
7. Optional: dictation on 13–25 via the SFSpeechRecognizer path; text size on those versions.
8. Per-version rows in TESTS.md.

### Intel (0.3)
- **CPU-only fix** for "crashed on the first question" ⏳ — most likely llama.cpp's Metal backend on an Intel/AMD GPU with every layer offloaded (no crash log from the Intel Mac). Intel builds load with 0 GPU layers and `offload_kqv` / `op_offload` off; log `[llm] loading model (CPU only — Intel)`. Verified under Rosetta with the real model (loads 11.7 s, replies, no crash) — Rosetta can't reproduce the Intel-GPU crash itself. The Metal device is still initialized on Intel but does no work.
- **Speed:** the x86_64 llama.cpp is built for baseline x86 (GGML_NATIVE/AVX/AVX2/FMA/F16C all OFF). Every Intel Mac that runs macOS 13+ (2017+) has AVX2 + FMA + F16C → enable them for the x86_64 build (needs per-target CMake flags for `llama-cpp-sys-2`).
- Hold-to-talk (SpeechAnalyzer) on Intel: unknown.

### Feature ideas (from the 2026-09-24 brainstorm, not started)
- Point at controls in any app (generalise the settings ring via AX).
- See the screen (needs image input below) — "what is this pop-up?", "is this a scam?"; ask before every capture, never keep it.
- Guided troubleshooting (no sound, Wi-Fi, printer, slow Mac) using `get_mac_info`.
- Scam and safety help.
- Do it for them (safe, reversible settings only, with a confirmation; never VoiceOver/passwords/deletion).
- Remote help from family (share a summary of where they got stuck).
- Remembering progress ("last week you set up larger text…").
- Use the Mac's built-in mic when Bluetooth headphones are connected (no call-mode switch → faster, keeps music quality).
- Software echo cancellation for meetings on speakers (WebRTC AEC3 / SpeexDSP with the captured system audio as reference) — macOS VPIO was tried and reverted (it ducked the system mic level).

### Image input via Gemma 4 multimodal (not started)
Download the `.mmproj` alongside the GGUF → load the vision projector → encode images (llama.cpp LLaVA API: `llava_image_embed_make_with_bytes`, `llava_eval_image_embed`) → feed embeddings before text → frontend sends image bytes from the resource chip.

### Detail panel — History and About tabs (not started)
The Memory panel exists (≡ button, `DetailPanel.tsx`); History and About/settings tabs from the original spec don't.

### Per-platform transcription backends (planned, not started)
Move macOS transcription out of inline `#[cfg]` in `transcription.rs` into a compile-time-selected `backend` module (`transcription/{macos,windows,linux}.rs`) with a neutral surface (`available / auth_status / request_auth / start(record_path) / stop / assets_*`) and a `deliver_segment(...)` seam; core (store, save, render, subject/summary, live file) stays shared. Follow `accessibility.rs`'s `mod mac` precedent. Real Windows (WASAPI + STT) / Linux (PipeWire + STT) engines are separate work.

---

## Known issues & gotchas (still relevant)

- **Permissions are tied to the code signature.** Ad-hoc signatures change every build → after installing a new build, Accessibility (and possibly Microphone) must be re-granted: remove AI Buddy from the list (–) and re-add, or `tccutil reset Accessibility com.aibuddy.app`. Fixed properly only by Developer ID signing. The dev binary runs under Terminal, so TCC attributes it to Terminal; dev speech-permission re-prompts after rebuilds are normal (`tccutil reset SpeechRecognition`).
- **Hardened runtime + ad-hoc signing = launch crash** ("cannot be opened because of a problem": library validation rejects the bundled dylibs, "different Team IDs"). Keep `hardenedRuntime: false` until Developer ID signing. Before sharing any build: `codesign --verify --deep --strict` and launch it once from Terminal (TESTS.md).
- **⚠️ Never add a second ggml-based crate** (whisper-rs etc.): two statically linked ggml copies collide (Gemma failed to load, Metal lost). Run any such engine in a separate process.
- **onnxruntime dylib name is hardcoded** (`libonnxruntime.1.17.1.dylib` in `tauri.conf.json` and `tauri.universal.conf.json`) — update both if sherpa-rs bumps onnxruntime.
- **CMake caches the deployment target** — if llama.cpp fails on `std::filesystem` after changing it: `rm -rf src-tauri/target/release/build/llama-cpp-sys-2-*`.
- **Quit after transcription**: `lib.rs` calls `libc::_exit(0)` on `RunEvent::Exit` because tearing down the Swift SpeechAnalyzer objects in `cxa_finalize` aborts. Quitting within ~4.5 s of Stop skips the subject rename (full content still saved).
- **Bluetooth mic start** takes 1.3–1.9 s (call-mode switch); starting the mic while a reply is being read aloud stalls the main thread ~0.6 s once.
- **macOS can't give a "one-click allow"** for Accessibility; the system prompt + Settings toggle is the only path.
- **`dump-settings-anchors.mjs`** refuses to save a dump with < 10 panes (System Settings sometimes answers while still launching) — just rerun.
- **Settings probe output must stay out of the repo** (it lists this Mac's apps, devices and networks).
- **Legacy (pre-26) transcription internals**: energy gate (RMS 0.008, 2 s hold; ~100–200 ms lost at onset), 50 s request rotation (`_rotate`), pending-flush at request ends (`_endRequestPending`, logs `pending resolved by …`).
- `onboarding_complete` exists but the model is missing → download is skipped; delete `~/Library/Application Support/com.aibuddy.app/onboarding_complete` to re-run onboarding.
- `<end_of_turn>` could still render if the rolling buffer misses it — a final `.replace(/<end_of_turn>|<start_of_turn>/g, "")` in `agent.ts` would be a cheap safety net.
- Droid `onMouseDown` races `startDragging()` and `save_frontmost_app()` — harmless.
- Orphaned `~/Library/Application Support/com.aibuddy.app/models/ggml-base.en.bin` (145 MB) from the removed whisper engine can be deleted.

---

## Resolved bugs — history and lessons

- **2026-10-09 Logs printed private content** → counts/timings only (`[hotkey]`, `[voice]`, `[greeting]`, transcript subject). File paths still logged (a saved transcript's name includes its subject).
- **2026-10-06 Accessibility loop on installed builds** → the bundle wasn't signed at all (identifier `aibuddy-<hash>`), so TCC never matched; fixed with ad-hoc bundle signing. The first signed build crashed at launch (hardened runtime) → `hardenedRuntime: false`. Verified with a 6 s launch.
- **2026-10-06 Intel: crash on first question** → CPU-only on x86_64 (⏳ on real Intel, see Backlog → Intel).
- **2026-10-05 ⌥Space didn't switch to Chat while transcribing** ✅ → switch tabs before awaiting `get_pending_text` (a failed await used to abort the handler). Root cause not proven.
- **2026-10-05 Chat flashed away and back when holding ⌥Space** ✅ → Tauri's `set_focus` ordered the window front before activating (behind the active app until activation); now `aibuddy_focus_window_keep_front`. A simulated test claimed focus wasn't taken — an artifact of synthetic key events.
- **2026-10-05 System ding on ⌥Space** ⏳ → ⌘C on a disabled Copy menu item; skipped for native text controls with no selection.
- **2026-10-05 No cue for hold-to-talk on the Transcript tab** ✅ → ⌥Space switches to Chat; red tab dots for live mic.
- **2026-09-29 ⌥Space slow (window, then "Listening")** ✅ → show first, capture in background; "getting ready" stage for slow mics.
- **2026-09-29 Two waiting animations** → only the typing dots remain.
- **2026-09-28 Hold-to-talk returned nothing on Bluetooth** ✅ → stale tap format after the headset's call-mode switch; tap with `format: nil` + rewire/restart on config change. Lesson: register the session before `engine.start()` — the change arrives ~50 ms later.
- **2026-09-28 Hold-to-talk dropped words before a pause** → (1) startup latency ~0.5–1.3 s (measured) → mic on key-down + prewarmed lane; (2) one in-progress slot → per-time-range segments (5 unit tests from real traces). Verified with `say`-generated speech through the real `AnalyzerLane`.
- **2026-09-24 Settings: repeat request didn't reopen the page** ✅ → the model imitated its earlier reply; prompt rules + `SETTINGS_CLAIM` nudge; first round not streamed for settings messages.
- **2026-09-24 Duplicated reply tail ("ck!ck!")** → `llm.rs` flushed the stop-sequence prefix twice; `tail` now cleared before `break`.
- **2026-09-24 Ring "disappearing" on repeat** — user error; kept the 3-reading grace period and `[settings_nav] ring #N` logs.
- **2026-06-22 Finalize status lost on tab switch; silent summary/diarization failures** → `TranscriptStore.processing`, map-reduce summaries, explicit "speakers unavailable" / "summary unavailable".
- **2026-06-18 Save crawled in the background** → App Nap; `ActivityGuard` activity assertion. Diarization at the wrong sample rate → resample to 16 kHz.
- **2026-06-17 Inline edits lost line breaks** → raw `<replace>` blocks instead of JSON strings; Gmail "read-only" → paste-over-selection fallback.
- **2026-06-14 Quit-time SIGABRT after transcription** → `_exit(0)` on exit. **Dylibs missing in Finder-launched app** → bundle in `Contents/Frameworks` + rpath.
- **2026-06-12 Text lost at speaker switches** → SFSpeechRecognizer allows one on-device task at a time; moved to SpeechAnalyzer on macOS 26.
- **2026-06-10 Gemma failed to load** → duplicate ggml from whisper-rs; removed whisper-rs.

---

## How to Run

```bash
# First time
brew install cmake
cd /Users/almithani/projects/smbsoft/aibuddy
npm install

# Dev
npm run tauri dev

# Release builds
npm run tauri build          # Apple Silicon
npm run build:universal      # Intel + Apple Silicon
```

---

## Key Files

| File | Purpose |
|------|---------|
| `src/lib/agent.ts` | Agent loop, system prompt (`FEATURES`, `TOOL_DOCS`, `LENGTH_RULES`), tool execution, settings nudge |
| `src/components/ChatPanel/ChatPanel.tsx` | Chat UI, ⌥Space handling, hold-to-talk cards, read-aloud, tabs + recording dots, permission message |
| `src/components/TranscriptPanel/TranscriptPanel.tsx` | Transcript UI: start/stop, live turns, status bar, send to chat |
| `src/components/DetailPanel/DetailPanel.tsx` | Memory panel (≡ button) |
| `src/components/Highlight/` | Orange ring window for the settings navigator |
| `src/lib/topicSearch.ts`, `settingsGuide.ts`, `howtoGuide.ts` | Shared ranking + settings/how-to catalog access |
| `src/lib/displayPrefs.ts`, `src/app.css` | Follow macOS display settings; theme tokens (dark, light, contrast, transparency, motion) |
| `src/lib/memory.ts` | Memory types + friendly labels |
| `src/onboarding/` | Onboarding flow (model download, accessibility) |
| `src-tauri/src/lib.rs` | Tauri setup, windows, ⌥Space handler, command registration |
| `src-tauri/src/llm.rs` | Model load (GPU / CPU-only on Intel), generation, stop sequences |
| `src-tauri/src/accessibility.rs` | AX selected text, inline edit, clipboard fallback, `PrevApp` |
| `src-tauri/src/voice.rs`, `voice.m` | Hold-to-talk state machine, read-aloud (AVSpeechSynthesizer), listen cue |
| `src-tauri/src/speech_analyzer.swift` | SpeechAnalyzer lanes (meetings + dictation), prewarm, device-change handling |
| `src-tauri/src/capture.m` | Meeting audio capture (ScreenCaptureKit + mic), legacy SFSpeechRecognizer lanes |
| `src-tauri/src/transcription.rs`, `diarization.rs`, `speech_assets.rs` | Transcript store/save/summary, speaker diarization, speech model assets |
| `src-tauri/src/settings_nav.rs`, `settings_nav.m` | `open_system_settings` (deep link, verify, relaunch, ring, state), `get_mac_info`, window helpers |
| `src-tauri/src/howto.rs` | Opens how-to apps from the catalog |
| `src-tauri/src/display_prefs.rs`, `display_prefs.m` | Read + watch macOS text size / contrast / transparency / motion |
| `src-tauri/src/greeting.rs` | Cached personalised greeting |
| `src-tauri/src/memory.rs` | SQLite memory (rules + settings) |
| `src-tauri/src/download.rs` | Model download |
| `src-tauri/build.rs` | Compiles the ObjC files + Swift engine, rpath |
| `src-tauri/tauri.conf.json`, `tauri.universal.conf.json` | Windows (onboarding, droid, chat, highlight), bundle/signing; universal override |
| `src-tauri/Info.plist` | Microphone + speech recognition usage text |
| `src-tauri/.cargo/config.toml` | macOS 26 SDK C++ header fix for llama.cpp |
| `scripts/build-settings-catalog.py`, `build-howto-catalog.py` | Sources of the settings (97) and how-to (47) catalogs |
| `scripts/dump-settings-anchors.mjs`, `settings-controls-probe.swift`, `settings-probe-pages.txt` | Dev checks: anchors, catalog/control verification, AX probe |
| `scripts/bump-version.mjs` | Version bump across the three manifests |
| `TESTS.md` | Automated tests, macOS dev checks, manual checklists |
