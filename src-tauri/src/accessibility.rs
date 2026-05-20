/// macOS accessibility layer using AXUIElement.
///
/// Windows (UIA) and Linux (AT-SPI) stubs are added in a later milestone.
/// All public commands return Ok(None) / Ok(()) on non-macOS platforms so
/// the TypeScript layer can always call them without platform guards.

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

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
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

    fn cf_str(s: &str) -> CFString {
        CFString::new(s)
    }

    /// Returns the focused AXUIElement, or null if none.
    /// Caller is responsible for CFRelease-ing the returned pointer.
    unsafe fn focused_element() -> Option<AXUIElementRef> {
        let system = AXUIElementCreateSystemWide();
        let attr = cf_str("AXFocusedUIElement");
        let mut focused: CFTypeRef = std::ptr::null_mut();
        let err = AXUIElementCopyAttributeValue(
            system,
            attr.as_concrete_TypeRef(),
            &mut focused,
        );
        CFRelease(system as *const c_void);
        if err != AX_SUCCESS || focused.is_null() {
            None
        } else {
            Some(focused as AXUIElementRef)
        }
    }

    /// Read a CFString attribute from an AXUIElement as a Rust String.
    unsafe fn read_string_attr(element: AXUIElementRef, attr: &str) -> Option<String> {
        let cf_attr = cf_str(attr);
        let mut value: CFTypeRef = std::ptr::null_mut();
        let err = AXUIElementCopyAttributeValue(
            element,
            cf_attr.as_concrete_TypeRef(),
            &mut value,
        );
        if err != AX_SUCCESS || value.is_null() {
            return None;
        }
        // Treat the returned CFTypeRef as a CFStringRef
        let s = CFString::wrap_under_create_rule(value as CFStringRef).to_string();
        Some(s)
    }

    pub fn is_trusted() -> bool {
        unsafe { AXIsProcessTrustedWithOptions(std::ptr::null()) }
    }

    pub fn get_focused_text_impl() -> Option<String> {
        unsafe {
            let el = focused_element()?;
            let text = read_string_attr(el, "AXValue");
            CFRelease(el as *const c_void);
            text
        }
    }

    pub fn set_focused_text_impl(text: &str) -> Result<(), String> {
        unsafe {
            let el = focused_element().ok_or("No focused element")?;
            let attr = cf_str("AXValue");
            let value = cf_str(text);
            let err = AXUIElementSetAttributeValue(
                el,
                attr.as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            );
            CFRelease(el as *const c_void);
            if err == AX_SUCCESS {
                Ok(())
            } else {
                Err(format!("AXUIElementSetAttributeValue failed: {err}"))
            }
        }
    }

    pub fn get_selected_text_impl() -> Option<String> {
        unsafe {
            let el = focused_element()?;
            let text = read_string_attr(el, "AXSelectedText");
            CFRelease(el as *const c_void);
            text
        }
    }

    pub fn replace_selected_text_impl(text: &str) -> Result<(), String> {
        unsafe {
            let el = focused_element().ok_or("No focused element")?;
            let attr = cf_str("AXSelectedText");
            let value = cf_str(text);
            let err = AXUIElementSetAttributeValue(
                el,
                attr.as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            );
            CFRelease(el as *const c_void);
            if err == AX_SUCCESS {
                Ok(())
            } else {
                Err(format!("AXUIElementSetAttributeValue failed: {err}"))
            }
        }
    }
}

// ── Tauri commands (public API) ──────────────────────────────────────────────

#[tauri::command]
pub fn check_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    return mac::is_trusted();
    #[cfg(not(target_os = "macos"))]
    return true; // AT-SPI / UIA — assume granted for now
}

#[tauri::command]
pub fn get_focused_text() -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    return Ok(mac::get_focused_text_impl());
    #[cfg(not(target_os = "macos"))]
    return Ok(None);
}

#[tauri::command]
pub fn set_focused_text(text: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return mac::set_focused_text_impl(&text);
    #[cfg(not(target_os = "macos"))]
    return Ok(());
}

#[tauri::command]
pub fn get_selected_text() -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    return Ok(mac::get_selected_text_impl());
    #[cfg(not(target_os = "macos"))]
    return Ok(None);
}

#[tauri::command]
pub fn replace_selected_text(text: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return mac::replace_selected_text_impl(&text);
    #[cfg(not(target_os = "macos"))]
    return Ok(());
}
