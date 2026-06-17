/// macOS accessibility layer using AXUIElement.
///
/// Windows (UIA) and Linux (AT-SPI) stubs are added in a later milestone.
/// All public functions return empty/Ok on non-macOS platforms.

#[cfg(target_os = "macos")]
mod mac {
    use core_foundation::{
        base::{CFRelease, CFTypeRef, TCFType},
        string::{CFString, CFStringRef},
    };
    use std::ffi::c_void;

    type AXUIElementRef = *mut c_void;
    type AXError = i32;
    const AX_SUCCESS: AXError = 0;

    // ── AX framework ──────────────────────────────────────────────────────────

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementSetAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: CFTypeRef,
        ) -> AXError;
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    }

    // ── CoreGraphics (key events) ─────────────────────────────────────────────

    type CGEventRef = *mut c_void;
    type CGEventSourceRef = *mut c_void;
    type CGKeyCode = u16;
    type CGEventFlags = u64;

    const KCG_HID_EVENT_TAP: u32 = 0;
    const KVK_ANSI_C: CGKeyCode = 0x08;
    const KVK_ANSI_V: CGKeyCode = 0x09;
    const KCG_EVENT_FLAG_MASK_COMMAND: CGEventFlags = 0x0010_0000;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: CGEventSourceRef,
            virtual_key: CGKeyCode,
            key_down: bool,
        ) -> CGEventRef;
        fn CGEventSetFlags(event: CGEventRef, flags: CGEventFlags);
        fn CGEventPost(tap: u32, event: CGEventRef);
        /// Deliver an event straight to a specific process (no app activation,
        /// no focus change) — used to paste into a background browser.
        fn CGEventPostToPid(pid: i32, event: CGEventRef);
    }

    // ── Objective-C runtime ──────────────────────────────────────────────────

    type ObjcId = *mut c_void;
    type Sel = *const c_void;

    extern "C" {
        fn objc_getClass(name: *const std::ffi::c_char) -> ObjcId;
        fn sel_registerName(name: *const std::ffi::c_char) -> Sel;
    }

    // All variants are the same underlying symbol; different signatures let
    // us call methods with different argument/return types without unsafe casts.
    #[allow(clashing_extern_declarations)]
    extern "C" {
        #[link_name = "objc_msgSend"] fn msg_id        (r: ObjcId, s: Sel) -> ObjcId;
        #[link_name = "objc_msgSend"] fn msg_i32       (r: ObjcId, s: Sel) -> i32;
        #[link_name = "objc_msgSend"] fn msg_i64       (r: ObjcId, s: Sel) -> i64;
        #[link_name = "objc_msgSend"] fn msg_cstr      (r: ObjcId, s: Sel) -> *const std::ffi::c_char;
        #[link_name = "objc_msgSend"] fn msg_id_id     (r: ObjcId, s: Sel, a: ObjcId) -> ObjcId;
        #[link_name = "objc_msgSend"] fn msg_id_cstr   (r: ObjcId, s: Sel, a: *const std::ffi::c_char) -> ObjcId;
        #[link_name = "objc_msgSend"] fn msg_i64_id_id (r: ObjcId, s: Sel, a: ObjcId, b: ObjcId) -> i64;
        #[link_name = "objc_msgSend"] fn msg_i32_id_id (r: ObjcId, s: Sel, a: ObjcId, b: ObjcId) -> i32;
        #[link_name = "objc_msgSend"] fn msg_id_i32    (r: ObjcId, s: Sel, a: i32) -> ObjcId;
        #[link_name = "objc_msgSend"] fn msg_void_u64  (r: ObjcId, s: Sel, a: u64);
    }

    macro_rules! sel {
        ($s:literal) => {
            sel_registerName(concat!($s, "\0").as_ptr() as *const std::ffi::c_char)
        };
    }
    macro_rules! cls {
        ($s:literal) => {
            objc_getClass(concat!($s, "\0").as_ptr() as *const std::ffi::c_char)
        };
    }

    // ── NSWorkspace ───────────────────────────────────────────────────────────

    /// Returns the PID of the frontmost app via NSWorkspace — no AX trust needed.
    pub fn get_frontmost_pid() -> Option<i32> {
        unsafe {
            let workspace = msg_id(cls!("NSWorkspace"), sel!("sharedWorkspace"));
            if workspace.is_null() { return None; }
            let app = msg_id(workspace, sel!("frontmostApplication"));
            if app.is_null() { return None; }
            let pid = msg_i32(app, sel!("processIdentifier"));
            if pid > 0 { Some(pid) } else { None }
        }
    }

    // ── NSPasteboard helpers ──────────────────────────────────────────────────

    const PB_STRING_TYPE: &str = "public.utf8-plain-text";

    fn ns_string(s: &str) -> ObjcId {
        unsafe {
            let c = std::ffi::CString::new(s).unwrap_or_default();
            msg_id_cstr(cls!("NSString"), sel!("stringWithUTF8String:"), c.as_ptr())
        }
    }

    fn ns_string_to_rust(ns: ObjcId) -> Option<String> {
        if ns.is_null() { return None; }
        unsafe {
            let ptr = msg_cstr(ns, sel!("UTF8String"));
            if ptr.is_null() { return None; }
            Some(std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned())
        }
    }

    fn clipboard_get() -> Option<String> {
        unsafe {
            let pb = msg_id(cls!("NSPasteboard"), sel!("generalPasteboard"));
            if pb.is_null() { return None; }
            ns_string_to_rust(msg_id_id(pb, sel!("stringForType:"), ns_string(PB_STRING_TYPE)))
        }
    }

    fn clipboard_set(text: &str) {
        unsafe {
            let pb = msg_id(cls!("NSPasteboard"), sel!("generalPasteboard"));
            if pb.is_null() { return; }
            let t = ns_string(PB_STRING_TYPE);
            let arr = msg_id_id(cls!("NSArray"), sel!("arrayWithObject:"), t);
            msg_i64_id_id(pb, sel!("declareTypes:owner:"), arr, std::ptr::null_mut());
            msg_i32_id_id(pb, sel!("setString:forType:"), ns_string(text), t);
        }
    }

    fn clipboard_change_count() -> i64 {
        unsafe {
            let pb = msg_id(cls!("NSPasteboard"), sel!("generalPasteboard"));
            if pb.is_null() { return 0; }
            msg_i64(pb, sel!("changeCount"))
        }
    }

    // ── ⌘C key event ─────────────────────────────────────────────────────────

    fn post_cmd_c() {
        unsafe {
            for key_down in [true, false] {
                let ev = CGEventCreateKeyboardEvent(std::ptr::null_mut(), KVK_ANSI_C, key_down);
                CGEventSetFlags(ev, KCG_EVENT_FLAG_MASK_COMMAND);
                CGEventPost(KCG_HID_EVENT_TAP, ev);
                CFRelease(ev as *const c_void);
            }
        }
    }

    /// Send ⌘V directly to a specific process (so it lands in the target app
    /// even though our chat window currently has focus).
    fn post_cmd_v_to_pid(pid: i32) {
        unsafe {
            for key_down in [true, false] {
                let ev = CGEventCreateKeyboardEvent(std::ptr::null_mut(), KVK_ANSI_V, key_down);
                CGEventSetFlags(ev, KCG_EVENT_FLAG_MASK_COMMAND);
                CGEventPostToPid(pid, ev);
                CFRelease(ev as *const c_void);
            }
        }
    }

    /// Bring the app with `pid` to the front (NSRunningApplication) so it
    /// reliably processes the synthetic paste into its focused field.
    fn activate_app(pid: i32) {
        unsafe {
            let app = msg_id_i32(
                cls!("NSRunningApplication"),
                sel!("runningApplicationWithProcessIdentifier:"),
                pid,
            );
            if app.is_null() {
                return;
            }
            // NSApplicationActivateIgnoringOtherApps = 1 << 1 = 2.
            msg_void_u64(app, sel!("activateWithOptions:"), 2);
        }
    }

    /// Replace the target app's current selection by pasting `text`. Used when
    /// the AX write is rejected — common for browser/web text fields (Gmail in
    /// Chrome), which are user-editable but don't honor AXSelectedText writes.
    /// Saves and restores the clipboard around the paste.
    fn paste_replace(text: &str, prev_pid: Option<i32>) -> Result<(), String> {
        let pid = prev_pid.ok_or("No target app to paste into")?;

        let original = clipboard_get();
        clipboard_set(text);
        // Bring the target forward and let activation + clipboard settle.
        activate_app(pid);
        std::thread::sleep(std::time::Duration::from_millis(120));
        post_cmd_v_to_pid(pid);
        // Wait for the target app to consume the clipboard before restoring it,
        // otherwise the restore could race the paste.
        std::thread::sleep(std::time::Duration::from_millis(350));
        if let Some(t) = original {
            clipboard_set(&t);
        }
        Ok(())
    }

    /// Clipboard fallback for apps (e.g. Firefox) that don't expose AXSelectedText.
    /// Simulates ⌘C, waits for the clipboard to change, reads it, then restores.
    fn get_selected_text_via_clipboard() -> (Option<String>, String) {
        let mut log = String::from("[clipboard fallback]\n");

        let original = clipboard_get();
        let count_before = clipboard_change_count();
        log.push_str(&format!("clipboard saved (changeCount={})\n", count_before));

        post_cmd_c();
        log.push_str("posted ⌘C\n");

        // Poll up to 500 ms for the clipboard to change
        let mut selected: Option<String> = None;
        for attempt in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if clipboard_change_count() != count_before {
                selected = clipboard_get();
                log.push_str(&format!(
                    "clipboard changed (attempt {}) → {:?}\n",
                    attempt + 1,
                    selected.as_deref().unwrap_or("(empty)")
                ));
                break;
            }
        }
        if selected.is_none() {
            log.push_str("clipboard did not change within 500 ms\n");
        }

        // Restore original clipboard contents
        if let Some(ref t) = original {
            clipboard_set(t);
            log.push_str("clipboard restored\n");
        } else {
            log.push_str("clipboard was empty before, leaving as-is\n");
        }

        (selected, log)
    }

    // ── AX helpers ────────────────────────────────────────────────────────────

    fn cf_str(s: &str) -> CFString { CFString::new(s) }

    unsafe fn focused_element(prev_pid: Option<i32>) -> Option<AXUIElementRef> {
        let root: AXUIElementRef = match prev_pid {
            Some(pid) => AXUIElementCreateApplication(pid),
            None => AXUIElementCreateSystemWide(),
        };
        let attr = cf_str("AXFocusedUIElement");
        let mut focused: CFTypeRef = std::ptr::null_mut();
        let err = AXUIElementCopyAttributeValue(root, attr.as_concrete_TypeRef(), &mut focused);
        CFRelease(root as *const c_void);
        if err != AX_SUCCESS || focused.is_null() { None } else { Some(focused as AXUIElementRef) }
    }

    pub fn is_trusted() -> bool {
        unsafe { AXIsProcessTrustedWithOptions(std::ptr::null()) }
    }

    /// Show macOS's own Accessibility prompt (the closest thing to an "Allow"
    /// dialog — Accessibility has no one-click grant; it deep-links to Settings).
    /// Returns the current trust state. No-op prompt if already trusted.
    pub fn prompt_for_trust() -> bool {
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::CFDictionary;
        // kAXTrustedCheckOptionPrompt's value is the string "AXTrustedCheckOptionPrompt".
        let key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
        let value = CFBoolean::true_value();
        let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
        unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as *const c_void) }
    }

    /// Tries AXSelectedText; falls back to clipboard ⌘C if the AX read fails
    /// or returns empty. Returns (text, diagnostic_log).
    pub fn get_selected_text_debug(prev_pid: Option<i32>) -> (Option<String>, String) {
        let mut log = String::new();
        unsafe {
            // Step 1 — root AX element
            let root: AXUIElementRef = match prev_pid {
                Some(pid) => { log.push_str(&format!("AXUIElementCreateApplication(pid={})\n", pid)); AXUIElementCreateApplication(pid) }
                None      => { log.push_str("AXUIElementCreateSystemWide()\n"); AXUIElementCreateSystemWide() }
            };

            // Step 2 — focused element
            let mut focused: CFTypeRef = std::ptr::null_mut();
            let err1 = AXUIElementCopyAttributeValue(root, cf_str("AXFocusedUIElement").as_concrete_TypeRef(), &mut focused);
            CFRelease(root as *const c_void);
            log.push_str(&format!("AXFocusedUIElement → err={}", err1));
            if err1 != AX_SUCCESS || focused.is_null() {
                log.push_str(&format!(" FAILED (null={})\n", focused.is_null()));
                let (t, cl) = get_selected_text_via_clipboard();
                log.push_str(&cl);
                return (t, log);
            }
            log.push_str(" OK\n");

            // Step 3 — AXSelectedText
            let mut value: CFTypeRef = std::ptr::null_mut();
            let err2 = AXUIElementCopyAttributeValue(focused as AXUIElementRef, cf_str("AXSelectedText").as_concrete_TypeRef(), &mut value);
            CFRelease(focused);
            log.push_str(&format!("AXSelectedText → err={}", err2));

            if err2 == AX_SUCCESS && !value.is_null() {
                let s = CFString::wrap_under_create_rule(value as CFStringRef).to_string();
                log.push_str(&format!(" OK → {:?}\n", s));
                if !s.is_empty() {
                    return (Some(s), log);
                }
                log.push_str("AXSelectedText was empty, trying clipboard\n");
            } else {
                log.push_str(&format!(" FAILED (null={}) → trying clipboard\n", value.is_null()));
            }

            let (t, cl) = get_selected_text_via_clipboard();
            log.push_str(&cl);
            (t, log)
        }
    }

    pub fn replace_selected_text_impl(text: &str, prev_pid: Option<i32>) -> Result<(), String> {
        unsafe {
            // 1. Clean AX write — works for native fields (Mail, TextEdit, …).
            if let Some(el) = focused_element(prev_pid) {
                let attr = cf_str("AXSelectedText");
                let value = cf_str(text);
                let err =
                    AXUIElementSetAttributeValue(el, attr.as_concrete_TypeRef(), value.as_CFTypeRef());
                CFRelease(el as *const c_void);
                if err == AX_SUCCESS {
                    return Ok(());
                }
            }
            // 2. AX unavailable/rejected — the field is usually still editable
            //    (browser/web inputs like Gmail), so paste over the selection.
            paste_replace(text, prev_pid)
        }
    }
}

// ── State ────────────────────────────────────────────────────────────────────

/// PID of the app that was frontmost when the hotkey was pressed.
pub struct PrevApp(pub std::sync::Mutex<Option<i32>>);

// ── Public API ────────────────────────────────────────────────────────────────

pub fn save_prev_app_pid(state: &PrevApp) {
    #[cfg(target_os = "macos")]
    {
        let own_pid = std::process::id() as i32;
        if let Some(pid) = mac::get_frontmost_pid() {
            if pid != own_pid {
                *state.0.lock().unwrap() = Some(pid);
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = state;
}

/// Tries AXSelectedText on the saved app; falls back to ⌘C clipboard.
/// Returns (selected_text, diagnostic_log).
pub fn capture_selected_text_debug(state: &PrevApp) -> (String, String) {
    #[cfg(target_os = "macos")]
    {
        let trusted = mac::is_trusted();
        let pid = *state.0.lock().unwrap();
        let own_pid = std::process::id() as i32;
        let header = format!("AX trusted: {}\nSaved PID: {:?}  own PID: {}\n", trusted, pid, own_pid);
        let (text, ax_log) = mac::get_selected_text_debug(pid);
        return (text.unwrap_or_default(), format!("{}{}", header, ax_log));
    }
    #[cfg(not(target_os = "macos"))]
    (String::new(), "non-macOS platform".to_string())
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn check_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    return mac::is_trusted();
    #[cfg(not(target_os = "macos"))]
    return true;
}

/// Show macOS's Accessibility permission prompt. Returns current trust state.
#[tauri::command]
pub fn prompt_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    return mac::prompt_for_trust();
    #[cfg(not(target_os = "macos"))]
    return true;
}

#[tauri::command]
pub fn replace_selected_text(text: String, state: tauri::State<'_, PrevApp>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let pid = *state.0.lock().unwrap();
        return mac::replace_selected_text_impl(&text, pid);
    }
    #[cfg(not(target_os = "macos"))]
    return Ok(());
}
