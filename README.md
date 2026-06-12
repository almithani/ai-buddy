# AI Buddy

A local-first AI assistant that lives as a small droid overlay on your desktop. Uses Gemma 4B running fully on-device — no cloud, no API keys.

## Prerequisites

- **Node.js** 18+
- **Rust** stable (`rustup` recommended)
- **macOS**: Xcode CLT — `xcode-select --install` (required by the llama.cpp Metal backend)

## Running in dev

```sh
npm install
npm run tauri dev
```

On first launch the onboarding flow walks you through granting Accessibility permission and downloading the model (~5 GB). After that the droid appears in the bottom-right corner.

**Global hotkey:** `⌥ Space` — captures your selected text and opens the chat panel.

## Building a release

```sh
npm run tauri build
```

Output is in `src-tauri/target/release/bundle/`:

| Platform | Artifact |
|----------|----------|
| macOS    | `.dmg`, `.app` |
| Windows  | `.msi`, `.exe` |
| Linux    | `.deb`, `.AppImage` |

### Bundling the model

By default the model is downloaded at runtime. To ship it inside the installer, place the GGUF file at:

```
src-tauri/resources/models/gemma-4-E4B-it-Q4_K_M.gguf
```

before running `tauri build`. The app checks that location first on startup.

## Testing a release build

The bundled app shares its data directory with dev builds (`~/Library/Application Support/com.aibuddy.app`), so by default it skips onboarding and reuses the already-downloaded model.

**Re-run onboarding only** (keeps the 5 GB model):

```sh
rm ~/Library/Application\ Support/com.aibuddy.app/onboarding_complete
open "src-tauri/target/release/bundle/macos/AI Buddy.app"
```

**Full fresh-install simulation** (what a new user experiences — onboarding, real model download, all permission prompts):

```sh
# Stash dev state — don't delete it, your dev setup needs it back
mv ~/Library/Application\ Support/com.aibuddy.app ~/Library/Application\ Support/com.aibuddy.app.devbackup

open "src-tauri/target/release/bundle/macos/AI Buddy.app"
```

Restore dev state when done:

```sh
rm -rf ~/Library/Application\ Support/com.aibuddy.app
mv ~/Library/Application\ Support/com.aibuddy.app.devbackup ~/Library/Application\ Support/com.aibuddy.app
```

macOS permission grants (Microphone, Speech Recognition, Screen Recording) are tracked per app bundle by TCC, not in the data folder — the bundled app prompts fresh the first time regardless. To re-test the prompts themselves:

```sh
tccutil reset Microphone com.aibuddy.app
tccutil reset SpeechRecognition com.aibuddy.app
tccutil reset ScreenCapture com.aibuddy.app
```

## Sharing the build

Send the `.dmg` from `src-tauri/target/release/bundle/dmg/`. Recipients need an **Apple Silicon Mac on macOS 13+**, and should know:

1. **The app is unsigned** — double-clicking shows "damaged or unverified developer". Right-click → **Open** → Open instead (or `xattr -cr "/Applications/AI Buddy.app"`).
2. First launch downloads ~5 GB (the chat model).
3. Expect permission prompts for Microphone and Speech Recognition; **Screen Recording** (for meeting-participant audio) is granted in System Settings → Privacy & Security, then relaunch the app.
