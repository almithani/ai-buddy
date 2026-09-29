// Reads the macOS display accessibility settings the chat follows: text size,
// Increase contrast, Reduce transparency, Reduce motion. Light/Dark is handled
// by CSS (prefers-color-scheme), which WKWebView tracks on its own.
#import <AppKit/AppKit.h>

// macOS 26's Accessibility → Display → Text size is a content-size category
// (as on iOS). A per-accessibility override lives in
// com.apple.universalaccess FontSizeCategory.global ("DEFAULT" when unset);
// otherwise the global UIPreferredContentSizeCategoryName applies. There's no
// public API or change notification for it, so Rust polls this.
static NSString *TextSizeCategory(void) {
    CFPreferencesAppSynchronize(CFSTR("com.apple.universalaccess"));
    CFPreferencesAppSynchronize(kCFPreferencesAnyApplication);

    NSDictionary *fsc = CFBridgingRelease(
        CFPreferencesCopyAppValue(CFSTR("FontSizeCategory"), CFSTR("com.apple.universalaccess")));
    NSString *global = [fsc isKindOfClass:[NSDictionary class]] ? fsc[@"global"] : nil;
    NSString *category = nil;
    if ([global isKindOfClass:[NSString class]] && ![global isEqualToString:@"DEFAULT"]) {
        category = global;
    } else {
        id v = CFBridgingRelease(
            CFPreferencesCopyAppValue(CFSTR("UIPreferredContentSizeCategoryName"), kCFPreferencesAnyApplication));
        category = [v isKindOfClass:[NSString class]] ? v : @"L";
    }
    // "UICTContentSizeCategoryXL" → "XL"
    NSString *prefix = @"UICTContentSizeCategory";
    return [category hasPrefix:prefix] ? [category substringFromIndex:prefix.length] : category;
}

void aibuddy_display_prefs(char *category, int category_len, int *contrast, int *transparency, int *motion) {
    @autoreleasepool {
        strlcpy(category, TextSizeCategory().UTF8String ?: "L", (size_t)category_len);
        NSWorkspace *ws = NSWorkspace.sharedWorkspace;
        *contrast = ws.accessibilityDisplayShouldIncreaseContrast;
        *transparency = ws.accessibilityDisplayShouldReduceTransparency;
        *motion = ws.accessibilityDisplayShouldReduceMotion;
    }
}

/// Calls `cb` whenever contrast/transparency/motion (or other display
/// accessibility options) change, so the chat updates immediately rather than
/// on the next poll.
void aibuddy_display_prefs_observe(void (*cb)(void)) {
    [NSWorkspace.sharedWorkspace.notificationCenter
        addObserverForName:NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification
                    object:nil
                     queue:nil
                usingBlock:^(__unused NSNotification *note) { cb(); }];
}
