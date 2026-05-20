# AI Buddy — Specification

---

## Overview

AI Buddy is a local-first AI assistant delivered as a single installable desktop app. It lives on
screen as a small animated robot character and helps non-technical users with everyday computer
chores — editing emails, summarizing documents, renaming files, and more. Everything runs
on-device. No accounts, no subscriptions, no cloud required.

---

## Principles

- **One install, zero prerequisites.** The user downloads a file, installs it, and it works.
- **Non-technical first.** If it requires a terminal, it's out of scope.
- **Minimal footprint.** Always-on-top but never in the way.
- **Transparent.** The droid shows what it's doing and why, in plain language.

---

## Tech Stack

| Layer            | Technology                                                  |
|------------------|-------------------------------------------------------------|
| Shell            | Tauri v2                                                    |
| UI               | React + CSS (runs in Tauri's WebView)                       |
| Agent logic      | TypeScript (hand-rolled tool-calling loop — no framework)   |
| LLM inference    | `llama-cpp-rs` Rust crate (in-process, no Ollama)           |
| Model            | Gemma 4 4B GGUF (~2.5 GB, downloaded once on first launch)  |
| Accessibility    | Rust crates, platform-native (see Accessibility Layer)      |
| Memory & storage | SQLite via `rusqlite`                                       |

**Nothing to install. Nothing to run in the background. One file.**

---

## Architecture

```
┌─────────────────────────────────────────────┐
│  Tauri App (single process)                 │
│                                             │
│  ┌─────────────┐    ┌─────────────────────┐ │
│  │  React UI   │◄──►│   Rust Backend      │ │
│  │  (WebView)  │    │                     │ │
│  │             │    │  • llama-cpp-rs      │ │
│  │  • Droid    │    │  • Accessibility    │ │
│  │  • Chat     │    │  • SQLite           │ │
│  │  • Detail   │    │  • Tool handlers    │ │
│  └─────────────┘    └─────────────────────┘ │
└─────────────────────────────────────────────┘
         │ Tauri commands (invoke)
         ▼
  TypeScript agent loop
  (tool selection → Rust tool call → LLM → response)
```

The TypeScript layer manages conversation state and decides which tools to call. The Rust backend
executes them — LLM inference, accessibility reads/writes, file system operations, SQLite. No
sidecar, no IPC to an external process.

---

## Deliverables

| Platform | Format                    | Size   |
|----------|---------------------------|--------|
| macOS    | `.dmg` (notarized `.app`) | ~30 MB |
| Windows  | `.exe` (NSIS installer)   | ~35 MB |
| Linux    | `.AppImage`               | ~40 MB |

Model stored in the OS app data directory on first launch:

- macOS: `~/Library/Application Support/aibuddy/models/`
- Windows: `%APPDATA%\aibuddy\models\`
- Linux: `~/.local/share/aibuddy/models/`

---

## First Launch Flow

### Step 1 — Welcome

```
        ( ͡• ͜ʖ ͡•)
       /  AI BUDDY \

  "Hi! I'm AI Buddy.
   I live on your screen and
   help you get things done."

        [ Let's go → ]
```

### Step 2 — Model Download

```
        (downloading...)

  "I need to download my brain.
   It's about 2.5 GB and only
   happens once. WiFi recommended!"

  [████████████████░░░░]  78%
   Downloading Gemma 4 · 1.9 GB of 2.5 GB

        [ Cancel ]
```

Progress shown inside the droid's chat panel. On completion, the droid plays a short "ready"
animation before moving to the next step.

### Step 3 — Accessibility Permission (macOS / Windows)

Shown **before** the OS system dialog appears, so the user isn't confused by it.

```
         (o_o)
        /  !!  \

  "One more thing — I need
   permission to read and type
   in other apps. That's how I
   edit your emails and help
   fill in forms."

  "When you click Continue, your
   Mac will ask you to confirm.
   Just click Allow!"

        [ Continue → ]
```

After the user clicks Continue, the OS accessibility dialog appears immediately.

On Linux (AT-SPI), no explicit permission is required — this step is skipped.

### Step 4 — Ready

```
         \(^▽^)/

  "All set! I'm ready when you are.
   Try typing me a message, or drop
   a file on me."

        [ Start using AI Buddy ]
```

Droid transitions to idle overlay state. Onboarding never shown again.

---

## UI — The Droid Overlay

### Layout

```
Screen corner (user-configurable: any of the 4 corners)

  ┌──────┐
  │  🤖  │  ← Droid character (always on top, ~80×80px)
  └──────┘
      │
      ▼ (expands on click or incoming message)
  ┌──────────────────────┐
  │ Chat panel           │  ← Normal window (coverable)
  │                      │
  │ User: clean this up  │
  │ Buddy: Done! ✓       │
  │                      │
  │ [________________]   │
  └──────────────────────┘
```

The droid widget is always on top. The chat panel that expands from it behaves like a normal
window — other apps can cover it.

### Droid States

| State       | Description                                              |
|-------------|----------------------------------------------------------|
| `idle`      | Subtle breathing loop, occasional blink                  |
| `listening` | Eyes widen, slight lean forward — user is typing         |
| `thinking`  | Gear or spark animation — waiting for LLM                |
| `working`   | Animated task icon + progress shimmer                    |
| `done`      | Quick nod or small celebration, then returns to idle     |
| `error`     | Confused expression, slight head tilt                    |

### Task Icons (shown on droid body during `working` state)

| Icon | Task               |
|------|--------------------|
| 📄   | Reading/writing text |
| 🗂️  | File system        |
| 📅   | Calendar           |
| ✉️   | Email              |
| 📊   | Spreadsheet        |
| 🎙️  | Transcription      |
| 🌐   | Network / web      |
| 💭   | LLM inference      |

### Chat Panel

- Back-and-forth text conversation
- Status updates while working: short, plain English ("Reading your email…", "Done.")
- Drag-and-drop target for files and folders
- `[⊞]` button opens the Detail Panel

### Detail Panel

- **Memory** — stored preferences, editable and deletable
- **History** — past conversations, searchable
- **Scheduled tasks** — recurring jobs (v0.2+)
- **About / settings** — model info, corner preference, reset

---

## Memory & Preferences

The user teaches the droid through natural language:

> "From now on, highlight action items in red in meeting notes."
> "Always save renamed files to my Desktop."
> "When I ask you to clean up an email, keep it under 150 words."

Rules are stored in SQLite and injected into the system prompt at task time. The user can view,
edit, and delete rules in the Detail Panel.

---

## Accessibility Layer

The foundation for inline editing — works in any text field in any app.

```
┌────────────────────────────────────────────┐
│  AccessibilityTool (Rust)                  │
│                                            │
│  get_focused_text() → String               │
│  set_focused_text(text: String)            │
│  get_selected_text() → String              │
│  replace_selected_text(text: String)       │
└────────────────────────────────────────────┘
         ▲              ▲              ▲
   macOS AXUIElement  Windows UIA   Linux AT-SPI
   (accessibility     (uiautomation  (atspi crate)
    crate)             crate)
```

Default behavior: operate on selected text if a selection exists; fall back to the full focused
field.

---

## GPU Acceleration

| Platform              | Backend         | Auto-detected |
|-----------------------|-----------------|---------------|
| macOS (Apple Silicon) | Metal           | Yes           |
| Windows (NVIDIA)      | CUDA            | Yes           |
| Windows / Linux (AMD) | ROCm            | Yes           |
| All (fallback)        | CPU             | Default       |

No user action required. The Detail Panel shows which mode is active.

---

## Agent Tool-Calling Loop

```typescript
async function runAgent(userMessage: string, context: Context) {
  const rules = await memory.getRelevantRules(userMessage)
  const messages = buildMessages(userMessage, context, rules)

  while (true) {
    const response = await llm.complete(messages)
    if (response.isToolCall) {
      const result = await tools.execute(response.toolName, response.toolArgs)
      messages.push(toolResult(result))
    } else {
      return response.text
    }
  }
}
```

Tools are Rust functions exposed to TypeScript via Tauri `invoke()` commands. No orchestration
framework — the loop is ~50 lines.

---

## Chore Tools

### MVP (v0.1)

| Tool                   | What it does                                        |
|------------------------|-----------------------------------------------------|
| `get_focused_text`     | Read text from the focused field in any app         |
| `set_focused_text`     | Write edited text back to the focused field         |
| `get_selected_text`    | Read only the user's highlighted selection          |
| `replace_selected_text`| Replace only the highlighted selection              |
| `read_file`            | Read a dropped file (text, PDF, image via OCR)      |
| `store_preference`     | Save a user preference rule to memory               |
| `get_preferences`      | Retrieve relevant rules for a given task            |

### v0.2

| Tool              | What it does                                                  |
|-------------------|---------------------------------------------------------------|
| `rename_files`    | Rename/sort files in a directory by described rules           |
| `summarize`       | Summarize a dropped file or directory of files                |
| `send_email`      | Send via SMTP (credentials stored in OS keychain)             |
| `google_calendar` | Add/edit/query events (OAuth, one-time setup)                 |
| `write_spreadsheet`| Read/write local .xlsx                                       |
| `google_sheets`   | Read/write Google Sheets (OAuth)                              |

### v0.3 (stretch)

| Tool              | What it does                                                  |
|-------------------|---------------------------------------------------------------|
| `transcribe_audio`| Live mic → transcript via bundled Whisper                     |
| `meeting_minutes` | Summarize transcript into structured notes                    |
| `query_meetings`  | Answer questions about past transcribed meetings              |
| `fill_form`       | Fill repetitive web forms via accessibility layer             |

---

## Out of Scope

| Item             | Reason                                                           |
|------------------|------------------------------------------------------------------|
| Model updates    | Add in v0.2 with a simple in-app prompt                         |
| Cloud sync       | Contradicts local-first principle                               |
| Mobile           | Accessibility APIs differ significantly                         |
| Multi-model      | One well-chosen model keeps UX simple                           |
| Browser extension| Accessibility APIs cover the same ground without a separate install |

---

## Decisions Log

| Question                        | Decision                                              |
|---------------------------------|-------------------------------------------------------|
| LLM model                       | Gemma 4 4B GGUF                                      |
| Orchestration framework         | None — hand-rolled TypeScript loop                   |
| Model serving                   | `llama-cpp-rs` in-process (no Ollama)                |
| GPU                             | CPU default, auto-upgrade if GPU detected            |
| Model updates                   | Out of scope for now                                 |
| Accessibility permission UX     | Friendly droid screen before OS dialog               |
| Inline editing approach         | OS accessibility APIs (AXUIElement / UIA / AT-SPI)   |
| Selected text vs full field     | Selected text preferred, full field as fallback      |
