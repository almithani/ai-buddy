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
