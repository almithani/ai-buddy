# AI Buddy — Claude Instructions

## TASKS.md

**Always keep `TASKS.md` current.** Update it as part of completing any task — not as a separate step at the end.

- When something starts working: move it to the **What Works** section.
- When a new bug or limitation is found: add it to **Known Issues / Quirks**.
- When a feature is abandoned or superseded: remove or archive the entry.
- When new work is planned or discussed: add it to **Unfinished Features** or a new section.
- Update the `Last updated` date at the top whenever you edit the file.

Do not wait to be asked. If you fix a bug, update TASKS.md in the same turn.

## TESTS.md

**Always keep `TESTS.md` current.** Update it in the same turn as the change — not as a separate step.

- When you add or change a feature: add or update its manual checklist items, and note any automated test you added (file + what it covers).
- When you add automated tests or a new check script: list them in the automated / dev-check sections with the command to run them.
- When you fix a bug: add a checklist item or test that would catch it coming back.
- When a feature is removed: remove its tests and checklist items.
- Update the `Last updated` date at the top whenever you edit the file.

## Project

This is a macOS desktop app built with Tauri 2 (Rust backend, React/TypeScript frontend).
See `TASKS.md` for current state, known issues, and key files.

## Code style

- No comments unless the WHY is non-obvious.
- No unsolicited refactors — fix what was asked, nothing else.
- Rust: standard `cargo fmt` style.
- ObjC: use ARC (`-fobjc-arc`), no manual retain/release.
- TypeScript: functional components, no class components.

## Running the app

```bash
npm run tauri dev
```
