// Dev helper: records every identified control on System Settings pages so
// the settings catalog can use real control ids and Apple's on-screen labels.
// Reads "<pane> <anchor>" lines (anchor may be "-") on stdin and prints one
// JSON object per page. Needs Accessibility permission for the terminal.
// Quits and relaunches System Settings for every page: deep links do not
// navigate away from some sub-pages (e.g. Text Size), so a fresh launch is the
// only reliable way to land on each page.
//   xcrun swiftc -O scripts/settings-controls-probe.swift -o /tmp/probe
//   /tmp/probe < scripts/settings-probe-pages.txt > /tmp/settings-controls.jsonl   (survey)
//   node scripts/dump-settings-anchors.mjs --pages | /tmp/probe > /tmp/catalog-controls.jsonl
//   node scripts/dump-settings-anchors.mjs --verify-controls /tmp/catalog-controls.jsonl
// Keep the output OUT of the repo: pages list this Mac's apps, devices and
// network names. Account pages are deliberately not in the page list.
import AppKit
import ApplicationServices

let bundleId = "com.apple.systempreferences"
let controlRoles: Set<String> = ["AXCheckBox", "AXSlider", "AXPopUpButton", "AXButton", "AXSwitch", "AXRadioButton"]

func attr(_ e: AXUIElement, _ n: String) -> AnyObject? {
    var v: AnyObject?
    return AXUIElementCopyAttributeValue(e, n as CFString, &v) == .success ? v : nil
}
func str(_ e: AXUIElement, _ n: String) -> String { (attr(e, n) as? String) ?? "" }

func frame(_ e: AXUIElement) -> CGRect? {
    guard let p = attr(e, "AXPosition"), let z = attr(e, "AXSize") else { return nil }
    var pt = CGPoint.zero, sz = CGSize.zero
    AXValueGetValue(p as! AXValue, .cgPoint, &pt)
    AXValueGetValue(z as! AXValue, .cgSize, &sz)
    return CGRect(origin: pt, size: sz)
}

// Checkboxes/sliders in System Settings rows have no title; the visible label
// is the static text on the same row (closest vertical centre, to the left).
func rowLabel(_ e: AXUIElement) -> String {
    for own in ["AXTitle", "AXDescription"] { let s = str(e, own); if !s.isEmpty { return s } }
    guard let f = frame(e), let parent = attr(e, "AXParent"), CFGetTypeID(parent) == AXUIElementGetTypeID() else { return "" }
    let kids = (attr(parent as! AXUIElement, "AXChildren") as? [AXUIElement]) ?? []
    var best: (String, CGFloat)? = nil
    for k in kids where str(k, "AXRole") == "AXStaticText" {
        let v = str(k, "AXValue")
        guard !v.isEmpty, let kf = frame(k), kf.minX < f.minX else { continue }
        let dy = abs(kf.midY - f.midY)
        if dy < 30, best == nil || dy < best!.1 { best = (v, dy) }
    }
    return best?.0 ?? ""
}

func scan(_ pid: pid_t) -> (count: Int, headings: [String], controls: [[String: String]]) {
    var q: [(AXUIElement, Int)] = [(AXUIElementCreateApplication(pid), 0)]
    var n = 0, headings: [String] = [], controls: [[String: String]] = []
    while !q.isEmpty && n < 8000 {
        let (e, d) = q.removeFirst(); n += 1
        let role = str(e, "AXRole"), id = str(e, "AXIdentifier")
        if role == "AXHeading" {
            let h = str(e, "AXDescription").isEmpty ? str(e, "AXTitle") : str(e, "AXDescription")
            if !h.isEmpty { headings.append(h) }
        }
        // Many panes (Bluetooth, Keyboard, Lock Screen, ...) have no AXIdentifier
        // on their controls; keep those too — the catalog matches them by label.
        // Unidentified buttons are skipped: mostly navigation/list noise.
        if controlRoles.contains(role), !(id.isEmpty && role == "AXButton"),
           !id.hasSuffix(".infoButton"), id != "go back" {
            controls.append(["id": id, "role": role, "label": rowLabel(e),
                             "value": attr(e, "AXValue").map { "\($0)" } ?? ""])
        }
        if d < 30, let kids = attr(e, "AXChildren") as? [AXUIElement] { kids.forEach { q.append(($0, d + 1)) } }
    }
    return (n, headings, controls)
}

while let line = readLine() {
    let parts = line.split(separator: " ", maxSplits: 1).map(String.init)
    guard parts.count == 2 else { continue }
    let (pane, anchor) = (parts[0], parts[1])
    NSRunningApplication.runningApplications(withBundleIdentifier: bundleId).forEach { $0.terminate() }
    for _ in 0..<20 where !NSRunningApplication.runningApplications(withBundleIdentifier: bundleId).isEmpty {
        Thread.sleep(forTimeInterval: 0.1)
    }
    NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:\(pane)\(anchor == "-" ? "" : "?\(anchor)")")!)

    // Wait until the element count stops changing (page fully rendered), max ~6 s.
    var last = -1, stable = 0
    var result = (count: 0, headings: [String](), controls: [[String: String]]())
    // Ignore the first second: a fresh launch briefly settles on its default
    // page before the deep link navigates.
    Thread.sleep(forTimeInterval: 1.0)
    for _ in 0..<30 {
        Thread.sleep(forTimeInterval: 0.25)
        guard let app = NSRunningApplication.runningApplications(withBundleIdentifier: bundleId).first else { continue }
        result = scan(app.processIdentifier)
        if result.count == last && result.count > 20 { stable += 1; if stable >= 3 { break } } else { stable = 0 }
        last = result.count
    }
    let out: [String: Any] = ["pane": pane, "anchor": anchor, "headings": Array(result.headings.prefix(6)), "controls": result.controls]
    let data = try! JSONSerialization.data(withJSONObject: out, options: [.sortedKeys])
    print(String(data: data, encoding: .utf8)!)
    fflush(stdout)
}
