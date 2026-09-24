"""Source for src/lib/settingsCatalog.json (the generated file is what the app reads).

Edit topics here, then:
    python3 scripts/build-settings-catalog.py
    node scripts/dump-settings-anchors.mjs --check

Control ids / labels and step wording come from probing macOS 26.6 System
Settings with scripts/settings-controls-probe.swift. Prefer control_id (the
control's AXIdentifier — language-independent); use control_label (the visible
row label, English only, prefix match) when the control has no identifier.
Leave both out when the page has no single control worth ringing.
"""
import json
from pathlib import Path

AX = "com.apple.Accessibility-Settings.extension"
PRIVACY = "com.apple.settings.PrivacySecurity.extension"
SOUND = "com.apple.Sound-Settings.extension"
DISPLAYS = "com.apple.Displays-Settings.extension"
DESKTOP = "com.apple.Desktop-Settings.extension"
TRACKPAD = "com.apple.Trackpad-Settings.extension"
KEYBOARD = "com.apple.Keyboard-Settings.extension"
LOCK = "com.apple.Lock-Screen-Settings.extension"
DATETIME = "com.apple.Date-Time-Settings.extension"
APPEARANCE = "com.apple.Appearance-Settings.extension"
WALLPAPER = "com.apple.Wallpaper-Settings.extension"
BATTERY = "com.apple.Battery-Settings.extension*BatteryPreferences"
TOUCHID = "com.apple.Touch-ID-Settings.extension*TouchIDPasswordPrefs"
SCREENTIME = "com.apple.Screen-Time-Settings.extension"
CONTROLCENTER = "com.apple.ControlCenter-Settings.extension"

TOPICS = []


def topic(id, title, category, pane, keywords, steps, anchor=None, control_id=None, control_label=None, caution=None):
    t = {"id": id, "title": title, "category": category, "pane": pane}
    if anchor:
        t["anchor"] = anchor
    t["keywords"] = keywords
    if control_id:
        t["controlId"] = control_id
    if control_label:
        t["controlLabel"] = control_label
    t["steps"] = steps
    if caution:
        t["caution"] = caution
    TOPICS.append(t)


# ── Seeing the screen ─────────────────────────────────────────────────────────
topic("voiceover", "VoiceOver (screen reader)", "vision", AX,
      ["screen reader", "read the screen aloud", "blind", "voiceover", "can't see the screen"],
      ["Find the 'VoiceOver' switch at the top of the page.",
       "Click the switch to turn it on.",
       "The Mac will start speaking what is on the screen."],
      anchor="AX_VOICEOVER_ENABLED", control_id="AX_VOICEOVER_ENABLED", control_label="VoiceOver",
      caution="VoiceOver changes how the keyboard and mouse behave. To turn it off at any time, hold the Command key and press F5 (on some keyboards, also hold the fn key).")
topic("zoom", "Zoom (magnify the screen)", "vision", AX,
      ["magnify", "zoom in", "enlarge screen", "magnifier", "make everything bigger"],
      ["Turn on 'Use keyboard shortcuts to zoom'.",
       "Now hold Option and Command and press the equals key (=) to zoom in.",
       "Hold Option and Command and press the minus key (-) to zoom out."],
      anchor="AX_FEATURE_ZOOM", control_id="AX_ZOOM_ENABLE_HOTKEYS", control_label="Use keyboard shortcuts to zoom")
topic("text_size", "Make text bigger", "vision", AX,
      ["bigger text", "larger font", "font size", "text too small", "hard to read", "reading size"],
      ["Drag the 'Preferred reading size' slider to the right to make text bigger.",
       "Below it you can choose a size for each app, like Mail or Messages.",
       "Click 'Done' when you're happy with it."],
      anchor="AX_FONT_SIZE", control_id="AX_FONT_SIZE_SLIDER", control_label="Preferred reading size")
topic("increase_contrast", "Increase contrast", "vision", AX,
      ["contrast", "washed out", "hard to see buttons", "edges", "faint"],
      ["Turn on 'Increase contrast'.",
       "Buttons and borders will get darker outlines so they stand out."],
      anchor="display", control_id="AX_INCREASE_CONTRAST", control_label="Increase contrast")
topic("reduce_transparency", "Reduce transparency", "vision", AX,
      ["transparency", "see through", "blurry background", "glass"],
      ["Turn on 'Reduce transparency'.",
       "See-through areas like the menu bar and sidebars will become solid."],
      anchor="display", control_id="AX_REDUCE_TRANSPARENCY", control_label="Reduce transparency")
topic("invert_colors", "Invert colors", "vision", AX,
      ["invert colors", "reverse colors", "white text on black", "negative"],
      ["Turn on 'Invert colors'.",
       "'Smart' keeps photos looking normal; 'Classic' inverts everything."],
      anchor="display", control_id="AX_INVERT_COLOR", control_label="Invert colors")
topic("color_filters", "Color filters (color blindness)", "vision", AX,
      ["color blind", "colour blind", "color filters", "grayscale", "black and white screen", "tint"],
      ["Turn on 'Color filters'.",
       "Pick a 'Filter type', for example Grayscale or one for red/green color blindness.",
       "Use 'Intensity' to make the effect stronger or weaker."],
      anchor="AX_DISPLAY_FILTER_ENABLED", control_id="AX_DISPLAY_FILTER_ENABLED", control_label="Color filters")
topic("pointer_size", "Bigger mouse pointer", "vision", AX,
      ["cursor", "mouse pointer", "arrow too small", "bigger pointer", "pointer size"],
      ["Drag the 'Pointer size' slider to the right to make the pointer bigger."],
      anchor="AX_CURSOR_SIZE", control_id="AX_CURSOR_SIZE", control_label="Pointer size")
topic("find_pointer", "Shake to find the pointer", "vision", AX,
      ["can't find the mouse", "lost the pointer", "where is the cursor", "shake mouse"],
      ["Turn on 'Shake mouse pointer to locate'.",
       "Now when you lose the pointer, wiggle the mouse or your finger quickly and it will grow for a moment."],
      anchor="AX_FIND_CURSOR", control_id="AX_FIND_CURSOR", control_label="Shake mouse pointer to locate")
topic("reduce_motion", "Reduce motion", "vision", AX,
      ["motion sickness", "dizzy", "animations", "reduce motion", "moving screen"],
      ["Turn on 'Reduce motion'.",
       "Sliding and zooming animations will be replaced with gentle fades."],
      anchor="AX_REDUCE_MOTION", control_id="AX_REDUCE_MOTION", control_label="Reduce motion")
topic("dim_flashing", "Dim flashing lights", "vision", AX,
      ["flashing lights", "strobe", "epilepsy", "seizure", "flicker"],
      ["Turn on 'Dim flashing lights'.",
       "Videos with flashing or strobing lights will be dimmed automatically."],
      anchor="AX_DIM_FLASHING", control_id="AX_DIM_FLASHING", control_label="Dim flashing lights")
topic("spoken_content", "Read selected text aloud", "vision", AX,
      ["read aloud", "speak text", "text to speech", "read to me", "read out loud", "out loud"],
      ["Turn on 'Speak selection'.",
       "Now highlight any text, then hold Option and press Escape to hear it read aloud.",
       "Change the voice with 'System voice'."],
      anchor="AX_FEATURE_SPOKENCONTENT", control_id="AX_SPOKEN_HOTKEY", control_label="Speak selection")
topic("speak_pointer", "Speak what's under the pointer", "vision", AX,
      ["read what I point at", "speak under mouse", "speak item under pointer"],
      ["Turn on 'Speak item under the pointer'.",
       "The Mac will read out whatever you rest the pointer on."],
      anchor="AX_SPOKEN_POINTER_ELEMENT", control_id="AX_SPOKEN_POINTER_ELEMENT", control_label="Speak item under the pointer")
topic("hover_text", "Hover Text (big text under the pointer)", "vision", AX,
      ["hover text", "magnify text under pointer", "enlarge word"],
      ["Turn on 'Hover Text'.",
       "Hold the Command key while pointing at text to see it large in a bubble."],
      anchor="AX_HOVER_TEXT_ENABLE", control_id="AX_HOVER_TEXT_ENABLE", control_label="Hover Text")
topic("accessibility_shortcut", "Accessibility shortcut", "vision", AX,
      ["accessibility shortcut", "quick turn on", "option command f5", "shortcut panel"],
      ["Tick the features you want in the shortcut, for example VoiceOver or Zoom.",
       "Then press Option, Command and F5 together at any time to turn them on or off."],
      anchor="AX_FEATURE_SHORTCUT")

# ── Hearing ───────────────────────────────────────────────────────────────────
topic("captions", "Subtitles and captions", "hearing", AX,
      ["subtitles", "captions", "closed captions", "sdh"],
      ["Turn on 'Prefer closed captions and SDH' so videos show subtitles when they have them.",
       "Pick a caption style from the list. 'Large Text' is easiest to read."],
      anchor="AX_FEATURE_CAPTIONS", control_id="AX_CAPTIONING_PREFER_SDH", control_label="Prefer closed captions and SDH")
topic("live_captions", "Live Captions (words on screen for any sound)", "hearing", AX,
      ["live captions", "transcribe", "show what people say", "deaf", "hard of hearing", "captions for calls"],
      ["Turn on 'Live Captions'.",
       "A small box will appear that shows spoken words as text, from calls, videos or the room.",
       "You can change the 'Font family' and colors below."],
      anchor="AX_SYSTEM_TRANSCRIPTION_ENABLED", control_id="AX_SYSTEM_TRANSCRIPTION_ENABLED", control_label="Live Captions")
topic("hearing_devices", "Hearing aids", "hearing", AX,
      ["hearing aid", "hearing device", "made for iphone hearing"],
      ["Make sure your hearing aids are on and nearby.",
       "Wait for them to appear in the list, then click their name to connect."],
      anchor="AX_FEATURE_HEARINGAIDS")
topic("flash_screen", "Flash the screen for alerts", "hearing", AX,
      ["flash screen", "visual alert", "can't hear alerts", "flash for notifications"],
      ["Turn on 'Flash the screen when an alert sound occurs'."],
      anchor="AX_FLASH_SCREEN", control_id="AX_FLASH_SCREEN", control_label="Flash the screen when an alert sound occurs")
topic("mono_audio", "Mono audio (one ear)", "hearing", AX,
      ["mono", "one ear", "deaf in one ear", "sound only in one side"],
      ["Turn on 'Play stereo audio as mono'.",
       "Both sides of your headphones will now play everything."],
      anchor="AX_MONO_AUDIO", control_id="AX_MONO_AUDIO", control_label="Play stereo audio as mono")
topic("background_sounds", "Background sounds (rain, ocean)", "hearing", AX,
      ["background sounds", "white noise", "rain sounds", "tinnitus", "calming sound", "ocean"],
      ["Turn on 'Background sounds'.",
       "Click 'Choose…' to pick a sound like Rain or Ocean.",
       "Use 'Background sounds volume' to set how loud it is."],
      anchor="AX_BACKGROUND_SOUNDS", control_id="AX_BACKGROUND_SOUNDS", control_label="Background sounds")
topic("headphone_accommodations", "Headphone Accommodations", "hearing", AX,
      ["headphone accommodations", "boost soft sounds", "tune headphones", "hearing boost"],
      ["Turn on 'Headphone Accommodations'.",
       "Click 'Customize Audio' to tune the sound to your hearing."],
      anchor="AX_PME_TOGGLE", control_id="AX_PME_TOGGLE", control_label="Headphone Accommodations")
topic("audio_descriptions", "Audio descriptions for videos", "hearing", AX,
      ["audio description", "describe video", "narrated video"],
      ["Turn on 'Play audio descriptions when available'."],
      anchor="AX_FEATURE_DESCRIPTIONS", control_id="AX_VIDEO_DESCRIPTION", control_label="Play audio descriptions when available")

# ── Keyboard, mouse and other ways to control the Mac ────────────────────────
topic("sticky_keys", "Sticky Keys (one key at a time)", "motor", AX,
      ["sticky keys", "press one key at a time", "can't hold two keys", "shortcuts hard"],
      ["Turn on 'Sticky Keys'.",
       "Now you can press keys like Command, then the letter, one after the other instead of together."],
      anchor="AX_STICKY_KEYS", control_id="AX_STICKY_KEYS", control_label="Sticky Keys")
topic("slow_keys", "Slow Keys (ignore accidental key presses)", "motor", AX,
      ["slow keys", "shaky hands", "accidental key presses", "tremor", "double letters"],
      ["Turn on 'Slow Keys'.",
       "Keys will only count when you hold them down a little longer."],
      anchor="AX_SLOW_KEYS", control_id="AX_SLOW_KEYS", control_label="Slow Keys")
topic("accessibility_keyboard", "On-screen keyboard", "motor", AX,
      ["on screen keyboard", "virtual keyboard", "type with the mouse", "keyboard on screen"],
      ["Turn on 'Accessibility Keyboard'.",
       "A keyboard will appear on screen that you can click with the mouse."],
      anchor="AX_VIRTUAL_KEYBOARD", control_id="AX_VIRTUAL_KEYBOARD", control_label="Accessibility Keyboard")
topic("full_keyboard_access", "Full Keyboard Access (no mouse)", "motor", AX,
      ["use without mouse", "keyboard only", "tab through buttons", "full keyboard access"],
      ["Turn on 'Full Keyboard Access'.",
       "Press Tab to move between buttons and Space to press them."],
      anchor="AX_FKA_ENABLE_CHECKBOX", control_id="AX_FKA_ENABLE_CHECKBOX", control_label="Full Keyboard Access")
topic("mouse_keys", "Mouse Keys (move pointer with keys)", "motor", AX,
      ["mouse keys", "move pointer with keyboard", "no mouse"],
      ["Turn on 'Mouse Keys'.",
       "Use the number keys (or the keys around 'I' on a laptop) to move the pointer."],
      anchor="AX_MOUSE_KEYS", control_id="AX_MOUSE_KEYS", control_label="Mouse Keys")
topic("double_click_speed", "Double-click speed", "motor", AX,
      ["double click", "double-click too fast", "can't double click", "slow down double click"],
      ["Drag the 'Double-click speed' slider to the left to give yourself more time between clicks."],
      anchor="AX_MOUSE_DOUBLE_CLICK_SPEED", control_id="AX_MOUSE_DOUBLE_CLICK_SPEED", control_label="Double-click speed")
topic("head_pointer", "Head pointer (move pointer with your head)", "motor", AX,
      ["head pointer", "head mouse", "control with head", "camera pointer"],
      ["Turn on 'Head pointer'.",
       "The camera will follow your head movements to move the pointer."],
      anchor="AX_HEAD_MOUSE", control_id="AX_HEAD_MOUSE", control_label="Head pointer")
topic("voice_control", "Voice Control (control the Mac by voice)", "motor", AX,
      ["voice control", "control by voice", "speak commands", "hands free", "talk to my computer"],
      ["Turn on 'Voice Control'.",
       "Say things like 'Open Mail' or 'Click Done'. Say 'Show commands' to see what you can say."],
      anchor="AX_VOICE_CONTROL_ENABLED", control_id="AX_VOICE_CONTROL_ENABLED", control_label="Voice Control")
topic("live_speech", "Live Speech (type to speak)", "motor", AX,
      ["type to speak", "can't talk", "live speech", "speak for me", "lost my voice"],
      ["Turn on 'Live Speech'.",
       "Press Option, Command and F5 and choose Live Speech, then type and press Return to have the Mac say it out loud."],
      anchor="AX_LIVE_SPEECH_ENABLED", control_id="AX_LIVE_SPEECH_ENABLED", control_label="Live Speech")
topic("switch_control", "Switch Control", "motor", AX,
      ["switch control", "adaptive switch", "single switch"],
      ["Turn on 'Switch Control'.",
       "Use 'Add' under Switches to set up the switch you use."],
      anchor="AX_SWITCH_CONTROL_ENABLE", control_id="AX_SWITCH_CONTROL_ENABLE", control_label="Switch Control")

# ── Connecting ────────────────────────────────────────────────────────────────
topic("wifi", "Wi-Fi", "connect", "com.apple.wifi-settings-extension",
      ["internet", "wifi", "wireless", "network", "not connected", "online"],
      ["Make sure the 'Wi-Fi' switch at the top is on (blue).",
       "Click the name of your network in the list.",
       "Type the Wi-Fi password when asked, then click 'Join'."],
      control_label="Wi-Fi")
topic("bluetooth", "Bluetooth (headphones, speakers, mouse)", "connect", "com.apple.BluetoothSettings",
      ["bluetooth", "headphones", "airpods", "wireless speaker", "wireless mouse", "pair", "connect"],
      ["Make sure the 'Bluetooth' switch at the top is on (blue).",
       "Put your device in pairing mode (see its instructions; often you hold its power button).",
       "When it appears under 'Nearby Devices', click 'Connect' next to it."],
      control_label="Bluetooth")
topic("airdrop", "AirDrop (share with nearby Apple devices)", "connect", "com.apple.AirDrop-Handoff-Settings.extension",
      ["airdrop", "send photo to iphone", "share nearby", "receive from iphone"],
      ["Click the 'AirDrop' menu.",
       "Choose 'Contacts Only' to receive from people you know, or 'Everyone for 10 Minutes' for anyone nearby."],
      anchor="AirDrop_Handoff", control_label="AirDrop")
topic("printers", "Printers and scanners", "connect", "com.apple.Print-Scan-Settings.extension",
      ["printer", "print", "add printer", "scanner", "printing not working"],
      ["Make sure the printer is on and connected to the same Wi-Fi.",
       "Click 'Add Printer, Scanner or Fax…'.",
       "Choose your printer from the list and click 'Add'."],
      anchor="print")

# ── Sound ─────────────────────────────────────────────────────────────────────
topic("sound", "Sound output and volume", "sound", SOUND,
      ["volume", "too quiet", "too loud", "speakers", "no sound", "mute", "output"],
      ["Under 'Output', click the speakers or headphones you want to use.",
       "Drag the 'Output volume' slider to the right to make it louder.",
       "Make sure 'Mute' is not ticked."],
      anchor="output", control_label="Output volume")
topic("alert_sound", "Alert sound", "sound", SOUND,
      ["alert sound", "beep", "error sound", "notification sound"],
      ["Click the 'Alert sound' menu and choose a sound you like.",
       "Use the 'Alert volume' slider to make alerts louder or softer."],
      control_id="AlertSoundPicker", control_label="Alert sound")
topic("startup_sound", "Startup chime", "sound", SOUND,
      ["startup sound", "chime when turning on", "boot sound"],
      ["Turn 'Play sound on startup' on or off."],
      control_id="BootChimeCheckbox", control_label="Play sound on startup")
topic("microphone_volume", "Microphone level", "sound", SOUND,
      ["microphone", "they can't hear me", "mic too quiet", "input volume"],
      ["Click 'Input', then choose the microphone you want to use.",
       "Drag the 'Input volume' slider to the right if people can't hear you."],
      anchor="input", control_id="InputVolumeSlider", control_label="Input volume")

# ── Screen ────────────────────────────────────────────────────────────────────
topic("brightness", "Screen brightness", "display", DISPLAYS,
      ["brightness", "screen too dark", "screen too bright", "dim", "brighter"],
      ["Drag the 'Brightness' slider right to make the screen brighter, or left to make it darker."],
      control_label="Brightness")
topic("auto_brightness", "Automatic brightness", "display", DISPLAYS,
      ["brightness keeps changing", "auto brightness", "screen dims by itself"],
      ["Turn 'Automatically adjust brightness' off if the screen keeps changing by itself."],
      control_label="Automatically adjust brightness")
topic("true_tone", "True Tone (warmer screen colors)", "display", DISPLAYS,
      ["true tone", "screen looks yellow", "screen color changes"],
      ["Turn 'True Tone' on or off. It makes colors look warmer in warm room light."],
      control_label="True Tone")
topic("night_shift", "Night Shift (warmer screen at night)", "display", DISPLAYS,
      ["night shift", "blue light", "screen too blue at night", "warm screen", "sleep better"],
      ["Scroll down and click 'Night Shift…'.",
       "Choose a 'Schedule', for example 'Sunset to Sunrise'.",
       "Drag 'Color temperature' toward 'More Warm' if you like."],
      anchor="nightShiftSection")
topic("screen_scale", "Make everything on screen bigger", "display", DISPLAYS,
      ["everything too small", "larger text resolution", "screen resolution", "icons too small", "make everything bigger"],
      ["Under the picture of your screen, click the option on the left, 'Larger Text'.",
       "Everything on screen gets bigger. Pick one to the right if it's too big."],
      anchor="resolutionSection")

# ── Look and feel ─────────────────────────────────────────────────────────────
topic("dark_mode", "Dark Mode / Light Mode", "appearance", APPEARANCE,
      ["dark mode", "light mode", "black background", "white background", "night mode"],
      ["At the top next to 'Appearance', click 'Light', 'Dark' or 'Auto'.",
       "'Auto' switches to dark in the evening."])
topic("scroll_bars", "Always show scroll bars", "appearance", APPEARANCE,
      ["scroll bars disappear", "scroll bar", "can't find scroll bar"],
      ["Next to 'Show scroll bars', choose 'Always'."],
      control_label="Always")
topic("sidebar_icon_size", "Bigger sidebar icons", "appearance", APPEARANCE,
      ["sidebar icons small", "finder sidebar", "icon size"],
      ["Click the 'Sidebar icon size' menu and choose 'Large'."],
      control_id="SidebarIconSizePicker", control_label="Sidebar icon size")
topic("wallpaper", "Wallpaper (desktop picture)", "appearance", WALLPAPER,
      ["wallpaper", "desktop picture", "background picture", "change background"],
      ["Click any picture to use it as your desktop picture.",
       "To use your own photo, click 'Add Photo' or 'Add Folder' at the top."],
      anchor="Wallpaper")
topic("screen_saver", "Screen saver", "appearance", WALLPAPER,
      ["screen saver", "screensaver"],
      ["Click a screen saver to choose it.",
       "To set when it starts, see the Lock Screen settings."],
      anchor="ScreenSaver")

# ── Desktop and Dock ──────────────────────────────────────────────────────────
topic("dock_size", "Dock size", "desktop", DESKTOP,
      ["dock too small", "dock icons", "bigger dock", "bottom bar"],
      ["Drag the 'Size' slider toward 'Large' to make the Dock icons bigger."],
      control_label="Dock Size")
topic("dock_magnification", "Dock magnification", "desktop", DESKTOP,
      ["dock magnification", "icons grow", "dock zoom"],
      ["Turn on 'Magnification'.",
       "Drag its slider to choose how big icons grow when you point at them."],
      control_label="Dock Magnification Size")
topic("dock_autohide", "Hide or show the Dock", "desktop", DESKTOP,
      ["dock disappeared", "dock hides", "dock missing", "hide dock"],
      ["Turn off 'Automatically hide and show the Dock' to keep the Dock always visible."],
      control_id="auto-hide-dock", control_label="Automatically hide and show the Dock")
topic("dock_position", "Move the Dock", "desktop", DESKTOP,
      ["dock position", "dock on the side", "move dock"],
      ["Click the 'Position on screen' menu and choose Left, Bottom or Right."],
      control_id="position", control_label="Dock position on screen")
topic("stage_manager", "Stage Manager", "desktop", DESKTOP,
      ["stage manager", "windows on the left", "windows grouped"],
      ["Turn 'Stage Manager' on or off. It keeps recent windows in a strip on the left."],
      anchor="StageManager", control_id="stage-manager-on", control_label="Stage Manager")
topic("default_browser", "Default web browser", "desktop", DESKTOP,
      ["default browser", "links open in wrong browser", "safari or chrome"],
      ["Click the 'Default web browser' menu and choose the browser you want links to open in."],
      control_id="default-web-browser", control_label="Default web browser")
topic("hot_corners", "Hot Corners", "desktop", DESKTOP,
      ["hot corners", "things happen in corner", "corner of screen"],
      ["Scroll to the bottom and click 'Hot Corners…'.",
       "Set a corner to '-' to stop it doing something when the pointer touches it."],
      anchor="HotCorners")

# ── Trackpad ──────────────────────────────────────────────────────────────────
topic("trackpad_speed", "Trackpad pointer speed", "trackpad", TRACKPAD,
      ["trackpad speed", "pointer too fast", "pointer too slow", "mouse too slow", "mouse too fast"],
      ["Drag the 'Tracking speed' slider left to slow the pointer down, or right to speed it up."],
      control_id="TrackingSpeedSlider", control_label="Tracking speed")
topic("tap_to_click", "Tap to click", "trackpad", TRACKPAD,
      ["tap to click", "clicks by accident", "tap instead of press"],
      ["Turn 'Tap to click' on to click with a light tap, or off to stop accidental clicks."],
      control_id="TapToClickToggle", control_label="Tap to click")
topic("right_click", "Right-click (secondary click)", "trackpad", TRACKPAD,
      ["right click", "secondary click", "control click", "two finger click"],
      ["Click the 'Secondary click' menu.",
       "'Click with Two Fingers' is the usual choice; you can also pick a corner."],
      control_id="SecondaryClickOptions", control_label="Secondary click")
topic("click_pressure", "Click firmness", "trackpad", TRACKPAD,
      ["click too hard", "trackpad click", "click pressure"],
      ["Drag the 'Click' slider to 'Light' if clicking takes too much pressure."],
      control_id="ClickPressureSlider", control_label="Click")
topic("scroll_direction", "Scroll direction", "trackpad", TRACKPAD,
      ["scroll direction", "scrolling backwards", "natural scrolling", "scrolls wrong way"],
      ["Click the 'Scroll & Zoom' tab at the top.",
       "Turn 'Natural scrolling' off if the page moves the opposite way you expect."])

# ── Keyboard and typing ───────────────────────────────────────────────────────
topic("key_repeat", "Key repeat", "keyboard", KEYBOARD,
      ["letters repeat", "keys repeating", "key repeat", "double letters"],
      ["Drag 'Key repeat rate' toward 'Off' or 'Slow'.",
       "Drag 'Delay until repeat' toward 'Long' so held keys don't repeat so quickly."],
      control_label="Key repeat rate")
topic("keyboard_brightness", "Keyboard backlight", "keyboard", KEYBOARD,
      ["keyboard light", "keys lit up", "backlight"],
      ["Turn 'Adjust keyboard brightness in low light' on or off."],
      control_label="Adjust keyboard brightness in low light")
topic("dictation", "Dictation (type by talking)", "keyboard", KEYBOARD,
      ["dictation", "type with voice", "speak instead of typing", "voice typing"],
      ["Turn on the 'Dictation' switch.",
       "In any place you can type, press the microphone key (or the 'Shortcut' shown) and start talking."],
      anchor="Dictation", control_label="Use Dictation wherever you can type text")
topic("text_replacements", "Text replacements", "keyboard", KEYBOARD,
      ["text replacement", "autocorrect", "shortcuts for typing", "expand text"],
      ["Click 'Text Replacements…'.",
       "Click '+' to add a short code that expands to a longer phrase, like your address."],
      anchor="TextReplacements")
topic("keyboard_language", "Add a keyboard language", "keyboard", KEYBOARD,
      ["another language keyboard", "input source", "type in french", "type in spanish"],
      ["Next to 'Input Sources', click 'Edit…'.",
       "Click '+' and choose the language you want to type in."],
      anchor="InputSources")

# ── Battery and power ─────────────────────────────────────────────────────────
topic("low_power_mode", "Low Power Mode", "battery", BATTERY,
      ["battery life", "battery drains", "save battery", "low power mode"],
      ["Click the 'Low Power Mode' menu and choose 'Only on Battery' or 'Always'."],
      control_id="low_power_mode", control_label="Low Power Mode")
topic("battery_options", "Battery health and charging", "battery", BATTERY,
      ["battery health", "optimized charging", "charging stops at 80", "battery not charging fully"],
      ["Click 'Options…' to see charging settings like 'Optimize Battery Charging'.",
       "Next to 'Battery Health', click the 'i' button to see its condition."],
      control_id="options", control_label="Options")
topic("display_sleep", "When the screen turns off", "battery", LOCK,
      ["screen turns off too fast", "screen goes black", "display sleep", "keep screen on", "sleep"],
      ["Click the 'Turn display off … when inactive' menu.",
       "Choose a longer time, like 30 minutes, so the screen stays on longer."],
      control_label="Turn display off")

# ── Security and sign-in ──────────────────────────────────────────────────────
topic("require_password", "Password after sleep", "security", LOCK,
      ["asks for password", "password after sleep", "lock screen password"],
      ["Click the 'Require password after screen saver begins or display is turned off' menu.",
       "Choose how long to wait before the Mac asks for your password."],
      control_label="Require password after screen saver begins")
topic("lock_screen_message", "Lock screen message", "security", LOCK,
      ["lock screen message", "if found call", "message on login screen"],
      ["Turn on 'Show message when locked'.",
       "Click 'Set…' and type something like 'If found, please call …'."],
      control_label="Show message when locked")
topic("touch_id", "Touch ID (fingerprint)", "security", TOUCHID,
      ["fingerprint", "touch id", "unlock with finger", "add fingerprint"],
      ["Click 'Add Fingerprint' and follow the steps, resting your finger on the Touch ID button.",
       "Make sure 'Use Touch ID to unlock your Mac' is on."],
      anchor="TouchID", control_label="Use Touch ID to unlock your Mac")
topic("change_password", "Change your login password", "security", TOUCHID,
      ["change password", "new password", "login password"],
      ["Next to 'Login password', click 'Change…'.",
       "Type your old password, then the new one twice."],
      anchor="Password")
topic("auto_login", "Log in automatically", "security", "com.apple.Users-Groups-Settings.extension",
      ["log in automatically", "skip password at startup", "automatic login"],
      ["Click the 'Automatically log in as' menu and choose your name, or 'Off'."],
      control_label="Automatically log in as",
      caution="With automatic login, anyone who turns on this Mac can use it without a password.")
topic("filevault", "FileVault (disk encryption)", "security", PRIVACY,
      ["filevault", "encrypt disk", "protect my files if stolen"],
      ["Scroll to 'FileVault' and click 'Turn On…'.",
       "Write down the recovery key somewhere safe."],
      anchor="FileVault",
      caution="If you forget your password and lose the recovery key, your files can't be recovered.")

# ── Privacy ───────────────────────────────────────────────────────────────────
topic("location_services", "Location Services", "privacy", PRIVACY,
      ["location", "gps", "apps know where i am", "location services"],
      ["Turn 'Location Services' on or off at the top.",
       "Below, turn it on or off for each app."],
      anchor="Privacy_LocationServices", control_label="Location Services")
topic("camera_access", "Which apps can use the camera", "privacy", PRIVACY,
      ["camera not working in zoom", "camera access", "webcam permission", "camera blocked"],
      ["Find the app in the list, like Zoom or FaceTime.",
       "Turn its switch on to let it use the camera. You may need to quit and reopen the app."],
      anchor="Privacy_Camera")
topic("microphone_access", "Which apps can use the microphone", "privacy", PRIVACY,
      ["microphone not working", "mic access", "they can't hear me in zoom", "microphone blocked"],
      ["Find the app in the list.",
       "Turn its switch on to let it use the microphone. You may need to quit and reopen the app."],
      anchor="Privacy_Microphone")
topic("screen_recording_access", "Screen sharing permission for apps", "privacy", PRIVACY,
      ["share my screen", "screen recording permission", "can't share screen in zoom"],
      ["Find the app in the list.",
       "Turn its switch on. The app will usually ask to quit and reopen."],
      anchor="Privacy_ScreenCapture")
topic("accessibility_access", "Apps allowed to control the Mac", "privacy", PRIVACY,
      ["accessibility permission", "allow app to control", "ai buddy permission"],
      ["Find the app in the list, for example AI Buddy.",
       "Turn its switch on. You may be asked for your password."],
      anchor="Privacy_Accessibility")

# ── General ───────────────────────────────────────────────────────────────────
topic("software_update", "Software updates", "general", "com.apple.Software-Update-Settings.extension",
      ["update", "upgrade macos", "is my mac up to date", "new version", "security update"],
      ["Wait a moment while the Mac checks for updates.",
       "If an update is listed, click 'Update Now' and keep the Mac plugged in."])
topic("storage", "Storage (free up space)", "general", "com.apple.settings.Storage",
      ["disk full", "storage", "free up space", "out of space", "running out of room"],
      ["Wait for the colored bar to show what's using space.",
       "Look through the recommendations and categories below to remove things you don't need."])
topic("about_this_mac", "About this Mac", "general", "com.apple.SystemProfiler.AboutExtension",
      ["what mac do i have", "macos version", "about this mac", "serial number", "how much memory"],
      ["Your Mac's name, chip, memory and macOS version are listed here."])
topic("date_time", "Date and time", "general", DATETIME,
      ["wrong time", "clock wrong", "date wrong", "set time"],
      ["Turn on 'Set time and date automatically' so the clock is always right."],
      control_label="Set time and date automatically")
topic("time_24h", "24-hour clock", "general", DATETIME,
      ["24 hour time", "am pm", "military time"],
      ["Turn '24-hour time' on or off."],
      control_label="24-hour time")
topic("time_zone", "Time zone", "general", DATETIME,
      ["time zone", "wrong hour", "travelling time"],
      ["Turn on 'Set time zone automatically using your current location', or pick a city below."],
      control_label="Set time zone automatically")
topic("language_region", "Language and region", "general", "com.apple.Localization-Settings.extension",
      ["language", "region", "change language", "date format", "temperature units", "fahrenheit", "celsius"],
      ["Click '+' under 'Preferred Languages' to add a language, then drag it to the top.",
       "Use 'Region', 'Date format' and the temperature buttons below for local formats."],
      control_id="region", control_label="Region")
topic("login_items", "Apps that open at startup", "general", "com.apple.LoginItems-Settings.extension",
      ["apps open at startup", "slow startup", "login items", "stop app opening automatically"],
      ["Under 'Open at Login', click an app and then the '–' button to stop it opening at startup."],
      anchor="startupItemsPref")
topic("time_machine", "Time Machine backups", "general", "com.apple.Time-Machine-Settings.extension",
      ["backup", "time machine", "back up my files", "restore files"],
      ["Plug in an external drive.",
       "Click 'Add Backup Disk…' and choose the drive. Backups then happen automatically."])

# ── Notifications, Focus, Siri, Screen Time ───────────────────────────────────
topic("notifications", "Notifications (pop-up messages)", "notifications", "com.apple.Notifications-Settings.extension",
      ["notifications", "pop ups", "alerts", "too many messages", "banners"],
      ["Scroll down to the list of apps and click the app that bothers you.",
       "Turn off 'Allow notifications' to stop its pop-ups, or pick a quieter style."])
topic("notification_previews", "Hide message previews", "notifications", "com.apple.Notifications-Settings.extension",
      ["message previews", "people can read my messages", "show previews"],
      ["Click the 'Show previews' menu and choose 'When Unlocked' or 'Never'."],
      control_id="show-previews", control_label="Show previews")
topic("do_not_disturb", "Do Not Disturb / Focus", "notifications", "com.apple.Focus-Settings.extension",
      ["do not disturb", "quiet", "silence notifications", "focus", "stop interruptions"],
      ["Click 'Do Not Disturb'.",
       "Add a schedule so it turns on by itself, for example overnight."])
topic("siri", "Siri", "notifications", "com.apple.Siri-Settings.extension*siri-sae",
      ["siri", "hey siri", "voice assistant"],
      ["Turn 'Siri' on.",
       "Use 'Listen for' to choose whether saying 'Siri' wakes it up."],
      control_label="Siri")
topic("screen_time_downtime", "Downtime (screen-free hours)", "notifications", SCREENTIME,
      ["downtime", "limit screen time", "bedtime"],
      ["Turn on 'Downtime'.",
       "Use 'Schedule' to choose the hours."],
      anchor="path=downtime", control_label="Downtime")
topic("battery_percentage", "Show battery percentage", "notifications", CONTROLCENTER,
      ["battery percentage", "how much battery left", "battery icon"],
      ["Find 'Battery' in the list.",
       "Turn on 'Show Percentage'."],
      anchor="Battery")

OUT = Path(__file__).resolve().parent.parent / "src/lib/settingsCatalog.json"
OUT.write_text(json.dumps(TOPICS, indent=2, ensure_ascii=False) + "\n")
print(f"{len(TOPICS)} topics → {OUT}")
