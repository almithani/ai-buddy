# AI Buddy — Tests

Last updated: 2026-10-05

How to check the app still works: automated tests first, then the macOS-dependent
dev checks, then manual checklists per feature. Record anything that fails as a
Known Issue in `TASKS.md`.

---

## 1. Automated (run on every change)

```bash
cd src-tauri && cargo test --lib   # Rust unit tests (21, ~1 s)
npx tsc --noEmit                   # TypeScript type check
```

| Area | File | What's covered |
|------|------|----------------|
| Echo dedup | `src-tauri/src/transcription.rs` (9 tests) | Drops "Me" segments that echo nearby "Them" speech (exact, subset, number-word and sub-word variants); keeps short utterances, far-apart and genuinely different ones; no-op with no "Them" |
| Greeting cache | `src-tauri/src/greeting.rs` (3 tests) | Output cleaning (strips quotes, rejects tags / >160 chars / blank); rules fingerprint changes when rules change |
| Settings catalog | `src-tauri/src/settings_nav.rs` (1 test) | `settingsCatalog.json` parses and topic ids are unique |
| How-to catalog | `src-tauri/src/howto.rs` (1 test) | `howtoCatalog.json` parses and topic ids are unique |
| Text size scaling | `src-tauri/src/display_prefs.rs` (2 tests) | Default category is unscaled; larger categories scale up, smaller down, capped at 2.0 |
| Hold-to-talk text | `src-tauri/src/voice.rs` (5 tests) | Speech before and after a pause is kept whatever order the recognizer finalizes it in (orders taken from real traces); everything heard is sent even if nothing was finalized; untimed results; nothing said → empty |

**Not covered automatically:** all frontend logic (no JS test runner yet): the agent loop, tool-call parsing, the settings-claim nudge, settings/how-to search ranking (`topicSearch.ts`), markdown-to-speech cleanup (`plainForSpeech`), and the paste/summarize heuristics.

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
- [ ] "make the text bigger" → still goes to the settings topic (open_settings), not a how-to
- [ ] "the website text is too small" → web_zoom how-to (Command-Plus) and mentions the related text-size setting

### Hold-to-talk voice (⌥Space)
- [ ] Tap ⌥Space quickly → chat window appears right away (well under a second, even over Chrome), no listening banner, no pop, nothing sent (the orange mic dot may flash briefly — expected). Terminal: `[hotkey] selection captured in …`
- [ ] Highlight text in Chrome, press ⌥Space → window appears immediately; a moment later the "summarize or edit?" offer appears with the text chip
- [ ] Highlight text in Terminal (in a window that isn't busy printing/redrawing), press ⌥Space → "summarize or edit?" offer with the text chip; terminal log shows `selection captured … N chars` with N > 0
- [ ] Hold ⌥Space, let go of ⌥ BEFORE Space, then press ⌥Space again later → the chat still responds (log may show `previous ⌥Space release was missed`)
- [ ] Hold ⌥Space over Terminal or Chrome → the chat appears and stays visible the whole time (no flash away and back)
- [ ] Tap ⌥Space over Terminal, then immediately type a few letters → they appear in the chat input, NOT in Terminal
- [ ] Press ⌥Space in Terminal / TextEdit / Notes with NOTHING selected → no system "ding" (only the pop when holding); log shows `skipping ⌘C fallback`
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

### Greeting
- [ ] No saved preferences → chat opens instantly with "What can I help you with?"
- [ ] "from now on call me Al and be playful" → terminal shows `[greeting] cached: …`; reopen chat → personal greeting appears instantly
- [ ] Delete that preference in the Memory window → next open shows the default again
- [ ] Memory window and `get_memory` do NOT show a `_greeting` entry
- [ ] ⌥Space with nothing highlighted → same instant greeting

### Chat and agent
- [ ] Reply length: "how do I take a screenshot", "make the text bigger", "what can you do?" → each answer ≤ ~3 short sentences/steps, no "Great question!" opener, no "let me know if…" ending
- [ ] "tell me more" after a short answer → more detail is given
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

### Onboarding and packaging
- [ ] Fresh onboarding (delete `~/Library/Application Support/com.aibuddy.app/onboarding_complete`) → model download, Accessibility step, restart button
- [ ] `npm run tauri build` → `.dmg` builds; installed app launches from Finder (bundled dylibs found)
