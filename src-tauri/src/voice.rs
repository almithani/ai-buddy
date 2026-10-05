//! Hold-to-talk on ⌥Space and spoken replies.
//!
//! Tap ⌥Space: the chat opens as before (handled in lib.rs). Hold it for
//! HOLD_DELAY: the chat shows "Listening…" (plus a "pop" cue) and the words
//! live. Release: the last words are waited for and `voice-result` is emitted —
//! the chat sends it straight away.
//!
//! The mic starts on key-down, not after HOLD_DELAY: people start talking the
//! moment they press, and starting the mic takes 150–770 ms (measured), so
//! waiting lost their first phrase. A tap discards the audio. When mic
//! permission hasn't been granted yet, it waits for a real hold instead, so a
//! tap never triggers the permission prompt.
//!
//! Events: voice-preparing (held, mic not live yet), voice-listening,
//! voice-partial {text}, voice-result {text},
//! voice-cancelled, voice-unavailable {reason: "macos" | "assets" | "mic"},
//! voice-blocked.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const HOLD_DELAY: Duration = Duration::from_millis(300);

struct State {
    key_down: bool,
    /// Bumped on every press so a stale hold thread can't act on a newer press.
    generation: u64,
    /// The hold passed HOLD_DELAY with the mic running: release sends.
    confirmed: bool,
    heard: Heard,
}

/// What's been heard so far. SpeechTranscriber reports each stretch of speech
/// as in-progress results that it later finalizes; after a pause a new stretch
/// can start before the previous one is final. Results are kept per time
/// range, so an earlier stretch is never overwritten by a later one — even if
/// releasing the key cuts finalization short.
#[derive(Default, Debug)]
struct Heard {
    segments: Vec<Segment>,
    /// Fallback for results that arrive without a time range.
    untimed_finals: String,
    untimed_partial: String,
}

#[derive(Debug)]
struct Segment {
    start: f64,
    text: String,
    is_final: bool,
}

/// Time-range jitter tolerated when comparing segment boundaries (seconds).
const EPS: f64 = 0.05;

impl Heard {
    fn update(&mut self, text: &str, is_final: bool, start: f64, end: f64) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if start < 0.0 || end < 0.0 {
            if is_final {
                append_words(&mut self.untimed_finals, text);
                self.untimed_partial.clear();
            } else {
                self.untimed_partial = text.to_string();
            }
            return;
        }
        if is_final {
            // A final covers every in-progress stretch that began before it ended.
            self.segments.retain(|s| s.is_final || s.start >= end - EPS);
        } else {
            // A newer in-progress result replaces in-progress ones from its start on.
            self.segments.retain(|s| s.is_final || s.start < start - EPS);
        }
        self.segments.push(Segment { start, text: text.to_string(), is_final });
        self.segments.sort_by(|a, b| a.start.total_cmp(&b.start));
    }

    fn text(&self) -> String {
        let mut out = String::new();
        for s in &self.segments {
            append_words(&mut out, &s.text);
        }
        append_words(&mut out, &self.untimed_finals);
        append_words(&mut out, &self.untimed_partial);
        out
    }
}

fn append_words(out: &mut String, text: &str) {
    if text.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(text);
}

static STATE: Mutex<State> = Mutex::new(State {
    key_down: false,
    generation: 0,
    confirmed: false,
    heard: Heard { segments: Vec::new(), untimed_finals: String::new(), untimed_partial: String::new() },
});
static APP: OnceLock<AppHandle> = OnceLock::new();
/// Set by the chat while it's generating a reply — holding ⌥Space then
/// doesn't listen (the answer would have nowhere to go).
static BLOCKED: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Clone)]
struct TextPayload {
    text: String,
}

#[derive(Serialize, Clone)]
struct ReasonPayload {
    reason: &'static str,
}

#[cfg(target_os = "macos")]
mod ffi {
    use std::ffi::{c_char, c_void};
    extern "C" {
        pub fn aibuddy_dictation_start(
            cb: extern "C" fn(i32, *const c_char, bool, f64, f64, *mut c_void),
            ctx: *mut c_void,
        ) -> i32;
        pub fn aibuddy_dictation_stop(
            done: extern "C" fn(*mut c_void),
            ctx: *mut c_void,
            cb: extern "C" fn(i32, *const c_char, bool, f64, f64, *mut c_void),
        );
        pub fn aibuddy_dictation_prewarm(
            cb: extern "C" fn(i32, *const c_char, bool, f64, f64, *mut c_void),
            ctx: *mut c_void,
        );
        pub fn aibuddy_mic_authorized() -> i32;
        pub fn aibuddy_speak(text: *const c_char);
        pub fn aibuddy_stop_speaking();
        pub fn aibuddy_play_listen_cue();
    }
}

/// Prepares a dictation lane in the background so the first hold starts fast.
pub fn prewarm() {
    #[cfg(target_os = "macos")]
    unsafe {
        ffi::aibuddy_dictation_prewarm(on_speech, std::ptr::null_mut())
    };
}

/// ⌥Space went down. Returns false for a repeat while already held, so the
/// caller only runs the "open chat" side once per press.
pub fn key_down(app: &AppHandle) -> bool {
    let _ = APP.set(app.clone());
    let generation = {
        let Ok(mut s) = STATE.lock() else { return false };
        if s.key_down {
            // Key repeat while held → ignore. But if Space isn't physically down,
            // a release was missed (e.g. ⌥ let go before Space); without this,
            // every later press would be ignored — no capture, no chat.
            if space_is_down() {
                return false;
            }
            eprintln!("[hotkey] previous ⌥Space release was missed — treating this as a new press");
        }
        s.key_down = true;
        s.confirmed = false;
        s.generation += 1;
        s.generation
    };
    stop_speaking();
    let app = app.clone();
    let pressed_at = Instant::now();
    std::thread::spawn(move || run_hold(&app, generation, pressed_at));
    true
}

/// ⌥Space went up. If the hold was confirmed, stop and send what was heard;
/// otherwise (a tap) the hold thread discards the audio.
pub fn key_up() {
    let send = {
        let Ok(mut s) = STATE.lock() else { return };
        s.key_down = false;
        std::mem::take(&mut s.confirmed)
    };
    if send {
        stop_dictation(true);
    }
}

fn space_is_down() -> bool {
    #[cfg(target_os = "macos")]
    {
        // kCGEventSourceStateCombinedSessionState = 0, kVK_Space = 49
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGEventSourceKeyState(state_id: i32, key: u16) -> bool;
        }
        unsafe { CGEventSourceKeyState(0, 49) }
    }
    #[cfg(not(target_os = "macos"))]
    true
}

fn still_held(generation: u64) -> bool {
    STATE.lock().map(|s| s.key_down && s.generation == generation).unwrap_or(false)
}

fn wait_for_hold(pressed_at: Instant) {
    let elapsed = pressed_at.elapsed();
    if elapsed < HOLD_DELAY {
        std::thread::sleep(HOLD_DELAY - elapsed);
    }
}

fn run_hold(app: &AppHandle, generation: u64, pressed_at: Instant) {
    if BLOCKED.load(Ordering::SeqCst) {
        wait_for_hold(pressed_at);
        if still_held(generation) {
            let _ = app.emit("voice-blocked", ());
        }
        return;
    }

    // The mic can take 1–2 s to go live (a Bluetooth headset switching into
    // call mode). If it isn't live when the hold is confirmed, say so right
    // away instead of showing nothing.
    {
        let app = app.clone();
        std::thread::spawn(move || {
            wait_for_hold(pressed_at);
            if let Ok(s) = STATE.lock() {
                if s.key_down && s.generation == generation && !s.confirmed {
                    let _ = app.emit("voice-preparing", ());
                }
            }
        });
    }

    // Start the mic right away when that can't pop a permission prompt;
    // otherwise only once it's clearly a hold.
    #[cfg(target_os = "macos")]
    let early = unsafe { ffi::aibuddy_mic_authorized() } == 1;
    #[cfg(not(target_os = "macos"))]
    let early = false;
    if !early {
        wait_for_hold(pressed_at);
        if !still_held(generation) {
            return;
        }
    }

    if let Ok(mut s) = STATE.lock() {
        s.heard = Heard::default();
    }
    #[cfg(target_os = "macos")]
    let rc = unsafe { ffi::aibuddy_dictation_start(on_speech, std::ptr::null_mut()) };
    #[cfg(not(target_os = "macos"))]
    let rc = -1;
    eprintln!("[voice] mic started {:?} after press (rc={rc})", pressed_at.elapsed());

    if rc != 0 {
        // Don't nag on a simple tap — only report when they're holding to talk.
        wait_for_hold(pressed_at);
        if still_held(generation) {
            let reason = match rc {
                -3 => "assets",
                -4 => "mic",
                _ => "macos",
            };
            eprintln!("[voice] dictation unavailable: {reason}");
            let _ = app.emit("voice-unavailable", ReasonPayload { reason });
        }
        return;
    }

    wait_for_hold(pressed_at);
    // Checked under the same lock key_up uses, so exactly one side stops the mic.
    // Emitted under the lock so it can't be overtaken by voice-preparing.
    let confirmed = {
        let Ok(mut s) = STATE.lock() else { return };
        let held = s.key_down && s.generation == generation;
        s.confirmed = held;
        if held {
            let _ = app.emit("voice-listening", ());
        }
        held
    };
    if confirmed {
        #[cfg(target_os = "macos")]
        unsafe {
            ffi::aibuddy_play_listen_cue()
        };
    } else {
        stop_dictation(false);
    }
}

/// `send` = emit the result (a confirmed hold); otherwise discard (a tap).
fn stop_dictation(send: bool) {
    let ctx = if send { 1usize } else { 0 } as *mut std::ffi::c_void;
    #[cfg(target_os = "macos")]
    unsafe {
        ffi::aibuddy_dictation_stop(on_dictation_done, ctx, on_speech)
    };
    #[cfg(not(target_os = "macos"))]
    on_dictation_done(ctx);
}

extern "C" fn on_speech(
    source: i32,
    text: *const std::ffi::c_char,
    is_final: bool,
    start: f64,
    end: f64,
    _ctx: *mut std::ffi::c_void,
) {
    if text.is_null() {
        return;
    }
    let text = unsafe { std::ffi::CStr::from_ptr(text) }.to_string_lossy().into_owned();
    if source != 2 {
        eprintln!("[voice] speech warning: {text}");
        return;
    }
    if is_final {
        eprintln!("[voice] final [{start:.2}–{end:.2}] {text}");
    }
    let heard = {
        let Ok(mut s) = STATE.lock() else { return };
        s.heard.update(&text, is_final, start, end);
        s.heard.text()
    };
    if let Some(app) = APP.get() {
        let _ = app.emit("voice-partial", TextPayload { text: heard });
    }
}

extern "C" fn on_dictation_done(ctx: *mut std::ffi::c_void) {
    let Some(app) = APP.get() else { return };
    if ctx.is_null() {
        let _ = app.emit("voice-cancelled", ());
        return;
    }
    let text = STATE.lock().map(|s| s.heard.text()).unwrap_or_default();
    eprintln!("[voice] sending: {text:?}");
    let _ = app.emit("voice-result", TextPayload { text });
}

#[tauri::command]
pub fn set_voice_blocked(blocked: bool) {
    BLOCKED.store(blocked, Ordering::SeqCst);
}

#[tauri::command]
pub fn speak_text(text: String) {
    #[cfg(target_os = "macos")]
    if let Ok(c) = std::ffi::CString::new(text) {
        unsafe { ffi::aibuddy_speak(c.as_ptr()) };
    }
    #[cfg(not(target_os = "macos"))]
    let _ = text;
}

#[tauri::command]
pub fn stop_speaking() {
    #[cfg(target_os = "macos")]
    unsafe {
        ffi::aibuddy_stop_speaking()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // Orders below are taken from real SpeechTranscriber traces (a phrase, a
    // pause, a second phrase), fed through the dictation lane.

    #[test]
    fn keeps_both_phrases_when_first_finalizes_before_second_starts() {
        let mut h = Heard::default();
        h.update("How do I", false, 0.0, 3.99);
        h.update("How do I take a screenshot?", false, 0.0, 4.04);
        h.update("How do I take a screenshot?", true, 0.0, 1.86);
        h.update("And save", false, 1.86, 5.27);
        h.update("And save it to my desktop.", true, 2.76, 5.27);
        assert_eq!(h.text(), "How do I take a screenshot? And save it to my desktop.");
    }

    #[test]
    fn keeps_first_phrase_when_second_starts_before_first_is_final() {
        let mut h = Heard::default();
        h.update("How do I take a screenshot", false, 0.0, 4.0);
        h.update("And save", false, 1.86, 5.0);
        assert_eq!(h.text(), "How do I take a screenshot And save");
        h.update("How do I take a screenshot?", true, 0.0, 1.86);
        h.update("And save it to my desktop.", true, 2.76, 5.27);
        assert_eq!(h.text(), "How do I take a screenshot? And save it to my desktop.");
    }

    #[test]
    fn nothing_final_still_sends_everything_heard() {
        // Finalization cut short at release: no finals at all.
        let mut h = Heard::default();
        h.update("I need to call my daughter", false, 0.0, 2.1);
        h.update("but the camera", false, 2.6, 4.0);
        h.update("but the camera is not working", false, 2.6, 4.4);
        assert_eq!(h.text(), "I need to call my daughter but the camera is not working");
    }

    #[test]
    fn untimed_results_fall_back_to_append() {
        let mut h = Heard::default();
        h.update("how do I", false, -1.0, -1.0);
        h.update("how do I take", true, -1.0, -1.0);
        h.update("a screenshot", false, -1.0, -1.0);
        assert_eq!(h.text(), "how do I take a screenshot");
    }

    #[test]
    fn empty_when_nothing_said() {
        assert_eq!(Heard::default().text(), "");
    }
}
