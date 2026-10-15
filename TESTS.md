# AI Buddy — Tests

Last updated: 2026-10-15

How to check the app still works: automated tests first, then the macOS-dependent
dev checks, then manual checklists per feature. Record anything that fails as a
Known Issue in `TASKS.md`.

---

## 1. Automated (run on every change)

```bash
cd src-tauri && cargo test --lib   # Rust unit tests (30, ~1 s)
npx tsc --noEmit                   # TypeScript type check
```

| Area | File | What's covered |
|------|------|----------------|
| Echo dedup | `src-tauri/src/transcription.rs` (9 tests) | Drops "Me" segments that echo nearby "Them" speech (exact, subset, number-word and sub-word variants); keeps short utterances, far-apart and genuinely different ones; no-op with no "Them" |
| Greeting cache | `src-tauri/src/greeting.rs` (3 tests) | Output cleaning (strips quotes, rejects tags / >160 chars / blank); rules fingerprint changes when rules change |
| Settings catalog | `src-tauri/src/settings_nav.rs` (1 test) | `settingsCatalog.json` parses and topic ids are unique |
| How-to catalog | `src-tauri/src/howto.rs` (1 test) | `howtoCatalog.json` parses and topic ids are unique |
| Text size scaling | `src-tauri/src/display_prefs.rs` (2 tests) | Default category is unscaled; larger categories scale up, smaller down, capped at 2.0 |
| How-to catalog extras | `src-tauri/src/howto.rs` (+2 tests) | Email guides have https `mail.google.com` Gmail versions; an unknown service variant is refused |
| Email drafts | `src-tauri/src/email.rs` (5 tests) | Percent-encoding; Gmail compose URL keeps its fixed base and can't be redirected by model text; mailto uses CRLF; reading "Subject:" + body (incl. `**Subject:**`, dropping `[Your Name]`); rejects drafts without subject/body |
| Model choice | `src-tauri/src/download.rs` (2 tests) | 8 GB RAM → Gemma 4 E2B; 16/18/24/32/64 GB → Gemma 4 E4B |
| Hold-to-talk text | `src-tauri/src/voice.rs` (5 tests) | Speech before and after a pause is kept whatever order the recognizer finalizes it in (orders taken from real traces); everything heard is sent even if nothing was finalized; untimed results; nothing said → empty |

**Not covered automatically:** all frontend logic (no JS test runner yet): the agent loop, tool-call parsing (JSON blocks and code-style calls like `open_settings(topic="x")`), the settings-claim nudge, settings/how-to search ranking (`topicSearch.ts`), markdown-to-speech cleanup (`plainForSpeech`), and the paste/summarize heuristics.

---

## 2. Dev checks (need a Mac with System Settings — run after catalog edits or a macOS update)

These drive the real System Settings app (it will quit and reopen repeatedly) and
need Accessibility permission for the terminal.

```bash
# 1. Regenerate the catalog and check every pane/anchor exists on this macOS
python3 scripts/build-settings-catalog.py
node scripts/dump-settings-anchors.mjs --check

# 2. Check every topic's control is actually on its page (~4 min)
xcrun swiftc -O scripts/settings-controls-probe.swift -o /tmp/probe
node scripts/dump-settings-anchors.mjs --pages | /tmp/probe > /tmp/catalog-controls.jsonl
node scripts/dump-settings-anchors.mjs --verify-controls /tmp/catalog-controls.jsonl

# 3. How-to guides: app bundle ids exist, settings cross-links resolve
python3 scripts/build-howto-catalog.py
node scripts/dump-settings-anchors.mjs --check-howtos
```

- Expected: `catalog OK: 97 topics…`, `all controls found`, `how-tos OK: 47 topics`.
- If the dump step says System Settings "returned only N pane(s)", it was still starting up: just rerun (the saved dump is left untouched).
- A control that's missing may just be a slow-loading page: re-probe only those pages before editing the catalog.
- Keep probe output out of the repo. It lists this Mac's apps, devices and networks.

---

## 3. Manual checklists

Run `npm run tauri dev` with the terminal visible (`[settings_nav]` and `[greeting]` logs appear there).

### Settings navigator

**Core flow** — the page opens, the control is ringed, and the reply states its current value:
- [ ] "the text is too small" → Text Size sub-page, "Preferred reading size" slider ringed, position mentioned
- [ ] "turn off tap to click" → Trackpad, "Tap to click" ringed, on/off mentioned
- [ ] "my bluetooth headphones won't connect" → Bluetooth switch ringed (label match)
- [ ] "turn on Wi-Fi" → Wi‑Fi switch ringed (non-breaking hyphen label)
- [ ] "how do I use my fingerprint" → "Use Touch ID to unlock your Mac" ringed (non-breaking space label)

**Already set / cautions**
- [ ] Ask to set VoiceOver to its current state → reply says it's already that way and skips steps
- [ ] "turn on VoiceOver" → reply mentions Command-F5 to turn it off

**No-ring pages** — opens with steps, and the reply doesn't mention a circle:
- [ ] "turn on dark mode"
- [ ] "which apps can use my camera"

**Repeat and recovery**
- [ ] Ask "change the brightness" twice → System Settings opens and is ringed both times; no text flashes and disappears
- [ ] "make the text bigger", stay on the sub-page, then "open Bluetooth" → Bluetooth opens (log: `relaunching System Settings`)

**Ring behaviour**
- [ ] Scroll System Settings → ring follows the control
- [ ] Switch to another app → ring disappears within ~1 s
- [ ] Click into the AI Buddy chat → ring stays; typing goes to the chat, not System Settings

**Edge cases**
- [ ] "change my ringtone" → says it can't help; no page opened
- [ ] "is my Mac up to date?" → macOS version via `get_mac_info` or Software Update opens
- [ ] "why is there no sound?" → checks volume/mute first
- [ ] Paste a long article and ask to summarize → streams normally (not held back)

**Step wording written from memory, not probed** — check against the screen:
- [ ] Night Shift "Schedule" · Displays "Larger Text" · Trackpad "Scroll & Zoom" → "Natural scrolling" · Control Center → Battery "Show Percentage"

### Easy reading (follows macOS display settings)
Toggle each in System Settings with the chat open; it should update within ~2 s.
- [ ] Accessibility → Display → Text size bigger → chat text AND window grow together (nothing cut off); back to default → shrinks back
- [ ] Increase contrast → muted grey text becomes near-white/black, borders get stronger
- [ ] Reduce transparency → tinted bubbles/chips become solid
- [ ] Reduce motion → typing dots, cursor blink, droid animation and the settings ring stop moving
- [ ] Appearance Light ↔ Dark → chat, Transcript tab and Memory panel switch themes (onboarding screens are not converted yet)

### How-to guides
- [ ] "how do I take a screenshot" → steps + "Shift-Command-3…" shortcut; Screenshot controls open
- [ ] "my app is frozen" → force-quit steps (Option-Command-Escape); no app opened
- [ ] "attach a photo to an email" → Mail opens, steps mention the paperclip
- [ ] "join my zoom meeting" → Zoom opens, or a friendly "isn't installed" note if Zoom is missing
- [ ] Gmail, nothing saved yet (Memory panel has no "Email:" row): "I want to send an email" → if Apple Mail is the default email app, Mail opens; otherwise the buddy asks "Mail app or Gmail?"
- [ ] Answer "Gmail" → Memory panel shows "Email: uses Gmail"; a Gmail compose window opens in the browser with Gmail steps (no Command-N shortcut mentioned) — in the SAME reply, without asking for a recipient
- [ ] "I want to send an email to my sister" (Gmail saved) → opens Gmail compose and gives steps; never asks for the address, subject or message
- [ ] Repeat 5× with different wordings ("draft an email in gmail…", "write a gmail to my son", "help me reply to my mail", "attach a photo to an email") → Gmail opens EVERY time, exactly once (not two tabs)
- [ ] "check my mail settings" / "what is email?" → does NOT open Gmail
- [ ] "draft an email in gmail which tells my sister I'll be late this evening" → status "Writing a draft…", then Gmail opens a new message with a subject + short message filled in (no made-up reasons, no name after the sign-off); chat says to add the address and click Send
- [ ] "send an email to bob@example.com asking if we can move lunch to Friday" → the draft opens with bob@example.com already in To
- [ ] Same with "I use Mail" saved → the draft opens in Apple Mail, filled in
- [ ] "I want to send an email" (nothing to write) → blank Gmail compose, no draft
- [ ] "how do I attach a photo" → Gmail compose opens, steps mention the paperclip "Attach files"; "how do I reply to an email" → Gmail inbox opens
- [ ] "I use the Mail app" (or delete the Memory row) → email guides open Mail again
- [ ] "make the text bigger" → still goes to the settings topic (open_settings), not a how-to
- [ ] "the website text is too small" → web_zoom how-to (Command-Plus) and mentions the related text-size setting

### Hold-to-talk voice (⌥Space)
- [ ] Tap ⌥Space quickly → chat window appears right away (well under a second, even over Chrome), no listening banner, no pop, nothing sent (the orange mic dot may flash briefly — expected). Terminal: `[hotkey] selection captured in …`
- [ ] Highlight text in Chrome, press ⌥Space → window appears immediately; a moment later the "summarize or edit?" offer appears with the text chip
- [ ] Highlight text in Terminal (in a window that isn't busy printing/redrawing), press ⌥Space → "summarize or edit?" offer with the text chip; terminal log shows `selection captured … N chars` with N > 0
- [ ] Hold ⌥Space, let go of ⌥ BEFORE Space, then press ⌥Space again later → the chat still responds (log may show `previous ⌥Space release was missed`)
- [ ] Hold ⌥Space over Terminal or Chrome → the chat appears and stays visible the whole time (no flash away and back)
- [ ] Tap ⌥Space over Terminal, then immediately type a few letters → they appear in the chat input, NOT in Terminal
- [ ] Press ⌥Space in Terminal / TextEdit / Notes with NOTHING selected → no system "ding" (only the pop when holding); log shows `selection captured in …: 0 chars` quickly (well under 0.5 s — no clipboard wait)
- [ ] Select text in TextEdit or Notes, press ⌥Space → still captured (summarize/edit offer)
- [ ] Select text on a web page in Chrome or Safari, press ⌥Space → still captured (uses the clipboard fallback)
- [ ] With Bluetooth headphones, hold ⌥Space → at ~⅓ s a GREY card says "Getting the microphone ready… keep holding", then it turns RED (red tint, red border, red title, pulsing red dot) with "Listening…" and the pop; text doesn't jump when it switches. Check in both Light and Dark mode.
- [ ] Let go while it still says "Getting the microphone ready" → "Keep holding ⌥Space until you hear the pop, then talk."
- [ ] Hold ⌥Space → after ~⅓ s the "Listening…" banner appears with a soft "pop" (first time: macOS asks for microphone permission)
- [ ] Press and START TALKING IMMEDIATELY (before the pop) → your first words are included (terminal: `[voice] mic started …ms after press`)
- [ ] Say a phrase, pause 1–2 s, say another, then release → both phrases are sent
- [ ] Say a phrase, pause, and release right after the second phrase → both phrases sent (terminal shows `sending: …` with both)
- [ ] **With Bluetooth headphones (e.g. AirPods):** hold ⌥Space and talk, 5+ times in a row → every attempt is transcribed (terminal: `audio device changed — restarted`, and the stop line shows a peak level well above 0.05)
- [ ] Switch between Bluetooth and the Mac's own speakers/mic between attempts → still works
- [ ] While holding, say "how do I take a screenshot" → words appear live in the banner
- [ ] Release → banner disappears, your words are sent as a message, the answer is **read aloud**
- [ ] Type a question instead → answer is NOT read aloud
- [ ] Hold and release without speaking → "I didn't catch that…"
- [ ] Release very quickly after the banner appears → mic indicator goes off (no stuck mic)
- [ ] Hold while an answer is still being written → header shows "Still answering…", no listening
- [ ] Highlight text in another app, hold ⌥Space, say "summarize this" → summary of the highlighted text
- [ ] Press ⌥Space while an answer is being read aloud → speech stops
- [ ] "always read your answers out loud" → typed questions are read too; "stop reading answers out loud" → nothing read; Memory window shows the setting
- [ ] Start a meeting transcription and stay on the Transcript tab, then hold ⌥Space and talk → the chat switches to the Chat tab and shows the grey → red listening card; both keep working (dictated words may also appear in the transcript as "Me" — same mic)
- [ ] Tap ⌥Space while on the Transcript tab → switches to the Chat tab
- [ ] Hold ⌥Space → the Chat tab label gets a pulsing red dot only once the card turns red ("Listening…"), not while grey; it disappears on release

### Greeting
- [ ] No saved preferences → chat opens instantly with "What can I help you with?"
- [ ] "from now on call me Al and be playful" → terminal shows `[greeting] cached: …`; reopen chat → personal greeting appears instantly
- [ ] Delete that preference in the Memory window → next open shows the default again
- [ ] Memory window and `get_memory` do NOT show a `_greeting` entry
- [ ] ⌥Space with nothing highlighted → same instant greeting

### Chat and agent
- [ ] Reply length: "how do I take a screenshot", "make the text bigger", "what can you do?" → each answer ≤ ~3 short sentences/steps, no "Great question!" opener, no "let me know if…" ending
- [ ] "tell me more" after a short answer → more detail is given
- [ ] "Can you transcribe meetings?" → yes, and says to use the Transcript tab (start/stop there); "where do my notes go?" → AI Buddy Transcripts in Documents (or the folder you set)
- [ ] "What can you do?" → short list covering transcription, talking (hold Option-Space), selected text, settings, everyday tasks, memory
- [ ] "How do I make you forget something?" → points to the Memory panel (≡ button)
- [ ] Summarize a pasted article → ≤5 short bullets
- [ ] Replies render markdown and end cleanly (no doubled last characters, no `<end_of_turn>`)
- [ ] While waiting for a reply, only the three-dots bubble shows (no blinking block cursor), including while a settings page or guide is opening; the dots disappear when the first words appear
- [ ] "from now on, always …" → preference stored and listed in the Memory window
- [ ] "save my transcripts to the Desktop" → `set_transcript_settings` updates the folder

### Inline editing
- [ ] Select text in Mail → click the droid → "clean this up" → text replaced in place, paragraphs kept
- [ ] Same in Gmail (Chrome) → replaced via the paste fallback; clipboard restored afterwards

### Summarize
- [ ] Paste >200 characters → resource chip + "summarize or edit?" offer
- [ ] ⌥Space over a highlight → same offer

### Transcription
- [ ] Start → speak → Stop: "Me"/"Them" turns appear live; file saved with summary + subject in the filename
- [ ] While recording, the Transcript tab label shows a pulsing red dot (also visible from the Chat tab); it disappears on Stop; reopening the chat mid-recording still shows it
- [ ] On speakers (no headphones), the other side's speech isn't duplicated as "Me"
- [ ] With Bluetooth headphones (e.g. AirPods): start a meeting transcription, talk for a minute → "Me" lines appear throughout (terminal shows `Mic: audio configuration changed — rebuilding engine` once near the start, then `Mic RMS` rising when you talk)
- [ ] Switch tabs during "Writing up your meeting notes…" → status bar survives
- [ ] Quit right after a session → no crash

### Low-memory Macs (8 GB)
- [ ] **On an 8 GB Mac with 0.2.0 already installed:** install the new build → onboarding opens again and downloads the smaller model (~3.1 GB, not 5 GB) → chat works; ask several questions without "Decode Error"/"ran out of memory"; `~/Library/Application Support/com.aibuddy.app/models/` then contains only `gemma-4-E2B-it-Q4_K_M.gguf`
- [ ] Fresh install on an 8 GB Mac → onboarding says "about 3.1 GB" and "Gemma 4 E2B"
- [ ] During any model download (incl. `npm run tauri dev`, where React StrictMode double-runs effects) the percentage and GB counter only ever go up — no jumping back and forth
- [ ] On the 8 GB Mac: "make the text bigger" opens Accessibility → Text Size (the small model's tool calls are understood); "how do I take a screenshot" gives the guide
- [ ] On a 16 GB+ Mac: nothing changes — still the ~5 GB model, no re-download
- [ ] Test the small model on a 16 GB Mac: `AIBUDDY_MODEL=e2b npm run tauri dev` → downloads E2B via onboarding; your E4B file must still be there afterwards

### Onboarding and packaging
- [ ] Fresh onboarding (delete `~/Library/Application Support/com.aibuddy.app/onboarding_complete`) → model download, Accessibility step, restart button
- [ ] Remove AI Buddy from Accessibility (–) and relaunch → macOS's permission prompt appears; the chat message's "Open Accessibility settings" link opens Privacy & Security → Accessibility; after switching it on, "restart AI Buddy" relaunches and the normal greeting appears
- [ ] Install a NEW build over an old one (unsigned) → expect lost Accessibility permission until removed + re-added (goes away once builds are Developer ID signed)
- [ ] `npm run tauri build` → `.dmg` builds; installed app launches from Finder (bundled dylibs found)
- [ ] `npm run build:universal` → `AI Buddy_<ver>_universal.dmg`; `lipo -archs` on `Contents/MacOS/aibuddy` and both `Contents/Frameworks/*.dylib` shows `x86_64 arm64`
- [ ] Logs contain no private content: after selecting text + ⌥Space, holding ⌥Space and talking, and a transcription save, the terminal shows only counts/timings (`… N chars`, `subject generated (N chars)`), never the selected text, what you said, the greeting or the meeting subject
- [ ] Built app actually launches: `codesign --verify --deep --strict "…/AI Buddy.app"` passes AND running `…/AI Buddy.app/Contents/MacOS/aibuddy` from Terminal for a few seconds shows no `Library not loaded` / code-signature errors (a signing change once made every launch fail)
- [ ] **On an Intel Mac:** installs and opens (right-click → Open while unsigned); onboarding + model download; model loads — run from Terminal and confirm `[llm] loading model (CPU only — Intel)`; asking a question does NOT crash and gets a reply (note speed); settings help + ring; ⌥Space tap + hold (does hold-to-talk work or say "needs macOS 26"?); meeting transcription
- [ ] On Apple Silicon after this change: Terminal shows `[llm] loading model (GPU)` and replies are as fast as before
