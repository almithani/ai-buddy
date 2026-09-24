// Finds, reads and tracks a control inside System Settings via the AX API so
// the settings navigator can report its state and draw a ring around it.
// Controls are matched by AXIdentifier when the catalog has one (these match
// the deep-link anchor names and are language-independent); otherwise by the
// row's visible label (English-only). Row labels are not on the control itself:
// System Settings checkboxes/sliders have no title, the label is the static
// text on the same row.
#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <os/lock.h>

static NSString *const kSettingsBundleId = @"com.apple.systempreferences";
static const NSInteger kMaxNodes = 8000;
static const NSInteger kMaxDepth = 30;

// The control currently ringed on screen; re-read every tick so the ring
// follows scrolling/window moves and disappears when the page changes.
// Guarded by gTrackedLock: set by the find command, read by the ring ticker.
static AXUIElementRef gTracked = NULL;
static os_unfair_lock gTrackedLock = OS_UNFAIR_LOCK_INIT;

static void SetTracked(AXUIElementRef el) {
    os_unfair_lock_lock(&gTrackedLock);
    if (gTracked) CFRelease(gTracked);
    gTracked = el;
    os_unfair_lock_unlock(&gTrackedLock);
}

static AXUIElementRef CopyTracked(void) {
    os_unfair_lock_lock(&gTrackedLock);
    AXUIElementRef el = gTracked ? (AXUIElementRef)CFRetain(gTracked) : NULL;
    os_unfair_lock_unlock(&gTrackedLock);
    return el;
}

static NSRunningApplication *SettingsApp(void) {
    return [NSRunningApplication runningApplicationsWithBundleIdentifier:kSettingsBundleId].firstObject;
}

static id CopyAttr(AXUIElementRef el, CFStringRef name) {
    CFTypeRef value = NULL;
    if (AXUIElementCopyAttributeValue(el, name, &value) != kAXErrorSuccess || !value) return nil;
    return CFBridgingRelease(value);
}

static NSString *StrAttr(AXUIElementRef el, CFStringRef name) {
    id v = CopyAttr(el, name);
    return [v isKindOfClass:[NSString class]] ? v : @"";
}

static BOOL FrameOf(AXUIElementRef el, CGRect *out) {
    id pos = CopyAttr(el, kAXPositionAttribute);
    id size = CopyAttr(el, kAXSizeAttribute);
    if (!pos || !size) return NO;
    CGPoint p; CGSize s;
    if (!AXValueGetValue((__bridge AXValueRef)pos, kAXValueTypeCGPoint, &p)) return NO;
    if (!AXValueGetValue((__bridge AXValueRef)size, kAXValueTypeCGSize, &s)) return NO;
    *out = (CGRect){p, s};
    return s.width > 0 && s.height > 0;
}

static NSString *RowLabel(AXUIElementRef el) {
    NSString *own = StrAttr(el, kAXTitleAttribute);
    if (own.length) return own;
    own = StrAttr(el, kAXDescriptionAttribute);
    if (own.length) return own;

    CGRect f;
    if (!FrameOf(el, &f)) return @"";
    id parent = CopyAttr(el, kAXParentAttribute);
    if (!parent) return @"";
    NSArray *kids = CopyAttr((__bridge AXUIElementRef)parent, kAXChildrenAttribute);
    NSString *best = @"";
    CGFloat bestDy = 30;
    for (id kid in kids) {
        AXUIElementRef k = (__bridge AXUIElementRef)kid;
        if (![StrAttr(k, kAXRoleAttribute) isEqualToString:(NSString *)kAXStaticTextRole]) continue;
        NSString *text = StrAttr(k, kAXValueAttribute);
        CGRect kf;
        if (!text.length || !FrameOf(k, &kf) || CGRectGetMinX(kf) >= CGRectGetMinX(f)) continue;
        CGFloat dy = fabs(CGRectGetMidY(kf) - CGRectGetMidY(f));
        if (dy < bestDy) { bestDy = dy; best = text; }
    }
    return best;
}

// Apple labels use typographic hyphens ("Wi‑Fi" has U+2011), curly quotes and
// non-breaking spaces ("Touch ID" has U+00A0).
static NSString *NormalizeLabel(NSString *s) {
    NSMutableString *m = [s.lowercaseString mutableCopy];
    for (NSString *dash in @[ @"\u2010", @"\u2011", @"\u2012", @"\u2013" ]) {
        [m replaceOccurrencesOfString:dash withString:@"-" options:0 range:NSMakeRange(0, m.length)];
    }
    [m replaceOccurrencesOfString:@"\u2019" withString:@"'" options:0 range:NSMakeRange(0, m.length)];
    for (NSString *space in @[ @"\u00a0", @"\u202f" ]) {
        [m replaceOccurrencesOfString:space withString:@" " options:0 range:NSMakeRange(0, m.length)];
    }
    return m;
}

static BOOL IsControlRole(NSString *role) {
    static NSSet *roles;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        roles = [NSSet setWithArray:@[ @"AXCheckBox", @"AXSlider", @"AXPopUpButton", @"AXButton",
                                       @"AXSwitch", @"AXRadioButton" ]];
    });
    return [roles containsObject:role];
}

// Breadth-first search of System Settings' AX tree. Returns a +1 element or NULL.
static AXUIElementRef FindControl(pid_t pid, NSString *controlId, NSString *controlLabel) {
    AXUIElementRef root = AXUIElementCreateApplication(pid);
    NSMutableArray *queue = [NSMutableArray arrayWithObject:@[ CFBridgingRelease(root), @0 ]];
    NSInteger visited = 0;
    while (queue.count && visited < kMaxNodes) {
        NSArray *entry = queue.firstObject;
        [queue removeObjectAtIndex:0];
        visited++;
        AXUIElementRef el = (__bridge AXUIElementRef)entry[0];
        NSInteger depth = [entry[1] integerValue];

        NSString *role = StrAttr(el, kAXRoleAttribute);
        if (IsControlRole(role)) {
            BOOL match = controlId.length
                ? [StrAttr(el, kAXIdentifierAttribute) isEqualToString:controlId]
                : [NormalizeLabel(RowLabel(el)) hasPrefix:NormalizeLabel(controlLabel)];
            if (match) return (AXUIElementRef)CFRetain(el);
        }
        if (depth < kMaxDepth) {
            for (id kid in CopyAttr(el, kAXChildrenAttribute) ?: @[]) {
                [queue addObject:@[ kid, @(depth + 1) ]];
            }
        }
    }
    return NULL;
}

static NSString *DescribeState(AXUIElementRef el) {
    NSString *role = StrAttr(el, kAXRoleAttribute);
    id value = CopyAttr(el, kAXValueAttribute);
    if ([role isEqualToString:@"AXCheckBox"] || [role isEqualToString:@"AXSwitch"]) {
        return [value respondsToSelector:@selector(intValue)] ? ([value intValue] ? @"switched on" : @"switched off") : @"";
    }
    if ([role isEqualToString:@"AXRadioButton"]) {
        return [value respondsToSelector:@selector(intValue)] && [value intValue] ? @"selected" : @"not selected";
    }
    if ([role isEqualToString:@"AXSlider"]) {
        id min = CopyAttr(el, kAXMinValueAttribute), max = CopyAttr(el, kAXMaxValueAttribute);
        if ([value isKindOfClass:[NSNumber class]] && [min isKindOfClass:[NSNumber class]] &&
            [max isKindOfClass:[NSNumber class]] && [max doubleValue] > [min doubleValue]) {
            double pct = ([value doubleValue] - [min doubleValue]) / ([max doubleValue] - [min doubleValue]) * 100.0;
            return [NSString stringWithFormat:@"slider at about %.0f%% of the way to the right", pct];
        }
        return @"";
    }
    if ([role isEqualToString:@"AXPopUpButton"] && [value isKindOfClass:[NSString class]]) {
        return [NSString stringWithFormat:@"currently set to \"%@\"", value];
    }
    return @"";
}

static void CopyOut(NSString *s, char *buf, int len) {
    if (!buf || len <= 0) return;
    strlcpy(buf, s.UTF8String ?: "", (size_t)len);
}

/// Waits up to timeout_ms for the control to appear. On success it becomes the
/// tracked control, is scrolled into view, its state is written to state_out
/// and its frame (global, top-left origin, points) to x/y/w/h.
/// Returns 1 found, 0 not found, -1 System Settings not running, -2 no AX trust.
int aibuddy_settings_find_control(const char *control_id, const char *control_label, int timeout_ms,
                                  char *state_out, int state_len,
                                  double *x, double *y, double *w, double *h) {
    @autoreleasepool {
        if (!AXIsProcessTrusted()) return -2;
        NSString *cid = control_id ? @(control_id) : @"";
        NSString *clabel = control_label ? @(control_label) : @"";
        if (!cid.length && !clabel.length) return 0;

        NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:timeout_ms / 1000.0];
        BOOL sawApp = NO;
        do {
            NSRunningApplication *app = SettingsApp();
            if (app) {
                sawApp = YES;
                AXUIElementRef el = FindControl(app.processIdentifier, cid, clabel);
                if (el) {
                    AXUIElementPerformAction(el, CFSTR("AXScrollToVisible"));
                    [NSThread sleepForTimeInterval:0.15];
                    CGRect f = CGRectZero;
                    FrameOf(el, &f);
                    CopyOut(DescribeState(el), state_out, state_len);
                    *x = f.origin.x; *y = f.origin.y; *w = f.size.width; *h = f.size.height;
                    SetTracked(el);
                    return 1;
                }
            }
            [NSThread sleepForTimeInterval:0.15];
        } while ([deadline timeIntervalSinceNow] > 0);
        return sawApp ? 0 : -1;
    }
}

/// Current frame of the tracked control. Returns 0 (and stops tracking) once
/// it is gone or off screen; the ring should then be hidden.
int aibuddy_settings_tracked_frame(double *x, double *y, double *w, double *h) {
    @autoreleasepool {
        AXUIElementRef el = CopyTracked();
        if (!el) return 0;
        CGRect f;
        BOOL ok = FrameOf(el, &f);
        CFRelease(el);
        if (!ok) {
            SetTracked(NULL);
            return 0;
        }
        *x = f.origin.x; *y = f.origin.y; *w = f.size.width; *h = f.size.height;
        return 1;
    }
}

void aibuddy_settings_stop_tracking(void) {
    SetTracked(NULL);
}

int aibuddy_settings_is_running(void) {
    return SettingsApp() != nil;
}

/// True when System Settings (or AI Buddy itself, e.g. the user reading the
/// chat) is frontmost — the ring should stay up in both cases.
int aibuddy_settings_ring_context_active(void) {
    @autoreleasepool {
        NSRunningApplication *front = NSWorkspace.sharedWorkspace.frontmostApplication;
        if (!front) return 0;
        if ([front.bundleIdentifier isEqualToString:kSettingsBundleId]) return 1;
        return front.processIdentifier == NSProcessInfo.processInfo.processIdentifier;
    }
}

/// Quits System Settings and waits (≤2 s) for it to exit. Deep links don't
/// navigate away from some sub-pages, so a relaunch is the reliable reset.
void aibuddy_settings_quit(void) {
    @autoreleasepool {
        aibuddy_settings_stop_tracking();
        for (NSRunningApplication *app in [NSRunningApplication runningApplicationsWithBundleIdentifier:kSettingsBundleId]) {
            [app terminate];
        }
        for (int i = 0; i < 20 && SettingsApp(); i++) [NSThread sleepForTimeInterval:0.1];
    }
}

/// Shows the ring window without making it key or activating AI Buddy, so
/// keyboard focus stays in System Settings.
void aibuddy_order_front_passive(void *ns_window) {
    [(__bridge NSWindow *)ns_window orderFrontRegardless];
}
