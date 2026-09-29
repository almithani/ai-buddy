import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// Mirrors DisplayPrefs in src-tauri/src/display_prefs.rs.
export interface DisplayPrefs {
  textSize: string;
  scale: number;
  increaseContrast: boolean;
  reduceTransparency: boolean;
  reduceMotion: boolean;
}

function apply(p: DisplayPrefs, scaleUi: boolean) {
  const root = document.documentElement;
  root.dataset.contrast = p.increaseContrast ? "more" : "normal";
  root.dataset.transparency = p.reduceTransparency ? "reduce" : "normal";
  root.dataset.motion = p.reduceMotion ? "reduce" : "normal";
  if (scaleUi) root.style.setProperty("--ui-scale", String(p.scale));
}

/// Keeps this window in step with macOS display settings. Only the chat scales
/// with text size — the droid and highlight windows have fixed sizes.
export function followDisplayPrefs(scaleUi: boolean) {
  invoke<DisplayPrefs>("get_display_prefs")
    .then((p) => apply(p, scaleUi))
    .catch(() => {});
  listen<DisplayPrefs>("display-prefs-changed", (e) => apply(e.payload, scaleUi));
}
