"""Source for src/lib/howtoCatalog.json (the generated file is what the app reads).

Everyday "how do I…" guides. Edit topics here, then:
    python3 scripts/build-howto-catalog.py
    node scripts/dump-settings-anchors.mjs --check-howtos

Fields: app = bundle id opened by `open_howto_app` (only ever from this file);
settings_topic = id of a related settings topic in build-settings-catalog.py.
Shortcuts are spelled out ("Shift-Command-5"): easier for seniors to follow and
reads naturally when replies are spoken aloud.
"""
import json
from pathlib import Path

TOPICS = []


def howto(id, title, category, keywords, steps, shortcut=None, app=None, settings_topic=None):
    t = {"id": id, "title": title, "category": category, "keywords": keywords, "steps": steps}
    if shortcut:
        t["shortcut"] = shortcut
    if app:
        t["app"] = app
    if settings_topic:
        t["settingsTopic"] = settings_topic
    TOPICS.append(t)


# ── Screenshots and recording ─────────────────────────────────────────────────
howto("screenshot", "Take a screenshot", "screen",
      ["screenshot", "screen shot", "capture the screen", "picture of my screen", "print screen"],
      ["Press Shift-Command-5 to show the screenshot controls at the bottom of the screen.",
       "Choose 'Capture Entire Screen', 'Capture Selected Window' or 'Capture Selected Portion'.",
       "Click 'Capture'. The picture is saved to your Desktop."],
      shortcut="Shift-Command-3 for the whole screen, Shift-Command-4 to drag around part of it",
      app="com.apple.screenshot.launcher")
howto("screen_recording", "Record a video of the screen", "screen",
      ["record screen", "screen recording", "video of my screen"],
      ["Press Shift-Command-5.",
       "Choose 'Record Entire Screen' or 'Record Selected Portion', then click 'Record'.",
       "To stop, click the stop button in the menu bar at the top of the screen. The video is saved to your Desktop."],
      shortcut="Shift-Command-5", app="com.apple.screenshot.launcher")

# ── Files and folders ─────────────────────────────────────────────────────────
howto("find_file", "Find a lost file", "files",
      ["find a file", "lost file", "where did my file go", "search for document", "can't find document"],
      ["Press Command-Space to open Spotlight search.",
       "Type part of the file's name or a word inside it.",
       "Use the arrow keys to pick it and press Return to open it."],
      shortcut="Command-Space")
howto("downloads", "Find something I downloaded", "files",
      ["downloads", "downloaded file", "where do downloads go", "find download"],
      ["Click the Finder icon (the blue face) in the Dock.",
       "Click 'Downloads' in the list on the left.",
       "Your newest downloads are usually at the top."],
      shortcut="Option-Command-L in Finder", app="com.apple.finder")
howto("rename_file", "Rename a file", "files",
      ["rename", "change file name"],
      ["Click the file once to select it.",
       "Press Return. The name becomes editable.",
       "Type the new name and press Return again."])
howto("new_folder", "Make a new folder", "files",
      ["new folder", "create folder", "organize files"],
      ["Open Finder and go to where you want the folder.",
       "Press Shift-Command-N.",
       "Type a name for the folder and press Return."],
      shortcut="Shift-Command-N", app="com.apple.finder")
howto("zip_files", "Zip or unzip files", "files",
      ["zip", "compress", "unzip", "open zip file", "archive"],
      ["To zip: hold the Control key and click the file or folder, then choose 'Compress'.",
       "To unzip: double-click the .zip file. The contents appear next to it."])
howto("empty_trash", "Empty the Trash", "files",
      ["empty trash", "delete permanently", "free up space trash"],
      ["Hold the Control key and click the Trash icon at the end of the Dock.",
       "Choose 'Empty Trash', then confirm."],
      settings_topic="storage")
howto("restore_from_trash", "Get something back from the Trash", "files",
      ["undelete", "restore deleted file", "deleted by mistake", "recover file"],
      ["Click the Trash icon at the end of the Dock.",
       "Find the item, hold the Control key and click it.",
       "Choose 'Put Back'. It returns to where it was."])
howto("eject_drive", "Safely remove a USB drive", "files",
      ["eject", "usb stick", "remove drive", "unplug memory stick", "external drive"],
      ["Open Finder.",
       "In the list on the left, click the eject symbol (⏏) next to the drive's name.",
       "When the drive disappears from the list, it's safe to unplug."],
      app="com.apple.finder")
howto("open_pdf_sign", "Open and sign a PDF", "files",
      ["sign pdf", "fill in form", "open pdf", "signature"],
      ["Double-click the PDF to open it in Preview.",
       "Click the pen-tip 'Markup' button at the top, then the signature button.",
       "Create a signature with your trackpad or camera, then click to place it."],
      app="com.apple.Preview")

# ── Apps and windows ──────────────────────────────────────────────────────────
howto("open_app", "Open an app", "apps",
      ["open app", "start program", "launch application", "find an app"],
      ["Press Command-Space, type the app's name and press Return.",
       "Or click the 'Apps' icon in the Dock to see all your apps and click one."],
      shortcut="Command-Space", app="com.apple.apps.launcher")
howto("quit_app", "Close a window vs quit an app", "apps",
      ["quit app", "close program", "app still running", "close window"],
      ["The red button at the top left of a window only closes that window.",
       "To quit the whole app, press Command-Q, or choose Quit from the app's menu at the top left."],
      shortcut="Command-Q to quit, Command-W to close a window")
howto("force_quit", "Force-quit a frozen app", "apps",
      ["frozen", "not responding", "stuck", "spinning wheel", "beach ball", "force quit", "app hangs"],
      ["Press Option-Command-Escape.",
       "Click the app that isn't responding.",
       "Click 'Force Quit'. Unsaved work in that app may be lost."],
      shortcut="Option-Command-Escape")
howto("switch_apps", "Switch between apps", "apps",
      ["switch apps", "go back to other program", "change window"],
      ["Hold Command and press Tab. Keep holding Command and press Tab again to move along.",
       "Let go when the app you want is highlighted."],
      shortcut="Command-Tab")
howto("exit_full_screen", "Get out of full screen", "apps",
      ["full screen", "exit full screen", "window fills screen", "menu bar gone"],
      ["Press the Escape key, or press Control-Command-F.",
       "Or move the pointer to the top of the screen and click the green button at the top left."],
      shortcut="Control-Command-F")
howto("find_lost_window", "Find a window that disappeared", "apps",
      ["lost window", "window disappeared", "minimized", "can't find my window", "hidden window"],
      ["Click the app's icon in the Dock — its windows come to the front.",
       "If it was minimized, click its small picture at the right end of the Dock.",
       "To see all open windows at once, press Control and the Up Arrow."],
      shortcut="Control-Up Arrow")
howto("delete_app", "Delete an app", "apps",
      ["uninstall", "delete app", "remove program"],
      ["Open Finder and click 'Applications' on the left.",
       "Drag the app onto the Trash in the Dock.",
       "Empty the Trash to finish."],
      app="com.apple.finder")
howto("update_apps", "Update my apps", "apps",
      ["update apps", "app updates", "app store updates"],
      ["Open the App Store.",
       "Click 'Updates' in the list on the left.",
       "Click 'Update All'."],
      app="com.apple.AppStore", settings_topic="software_update")

# ── Typing and text ───────────────────────────────────────────────────────────
howto("copy_paste", "Copy and paste", "text",
      ["copy", "paste", "cut", "copy and paste", "move text"],
      ["Select the text by dragging over it.",
       "Press Command-C to copy (or Command-X to cut).",
       "Click where you want it and press Command-V to paste."],
      shortcut="Command-C, then Command-V")
howto("undo", "Undo a mistake", "text",
      ["undo", "take back", "oops", "made a mistake", "deleted text"],
      ["Press Command-Z right away to undo the last thing you did.",
       "Press it again to undo more. Shift-Command-Z redoes."],
      shortcut="Command-Z")
howto("select_all", "Select everything", "text",
      ["select all", "highlight everything"],
      ["Click in the document or list, then press Command-A."],
      shortcut="Command-A")
howto("find_on_page", "Find a word on a page", "text",
      ["find word", "search page", "find in document"],
      ["Press Command-F.",
       "Type the word. Matches are highlighted; press Return to go to the next one."],
      shortcut="Command-F")
howto("emoji", "Type an emoji or symbol", "text",
      ["emoji", "smiley", "symbols", "special characters"],
      ["Click where you're typing.",
       "Press the Globe key (fn) and E together, or Control-Command-Space.",
       "Click an emoji to insert it."],
      shortcut="Globe-E")
howto("accents", "Type accented letters", "text",
      ["accent", "é", "foreign letters", "umlaut"],
      ["Hold down the letter key, for example E.",
       "A small menu of accented letters appears; click one or press its number."])
howto("save_document", "Save my work", "text",
      ["save", "save document", "save file"],
      ["Press Command-S.",
       "The first time, type a name, pick a place like 'Documents', and click 'Save'."],
      shortcut="Command-S")
howto("spell_check", "Fix a spelling mistake", "text",
      ["spelling", "red underline", "spell check"],
      ["Hold the Control key and click the underlined word.",
       "Choose the right spelling at the top of the menu."])

# ── Email and messages ────────────────────────────────────────────────────────
howto("email_attach", "Attach a photo or file to an email", "email",
      ["attach", "attachment", "send photo by email", "add picture to email"],
      ["In Mail, start a new message with Command-N.",
       "Click the paperclip button at the top of the message.",
       "Pick the photo or file and click 'Choose File'. You can also drag it into the message."],
      app="com.apple.mail")
howto("email_reply", "Reply to an email", "email",
      ["reply", "answer email", "respond to email"],
      ["Click the email you want to answer.",
       "Click the curved arrow 'Reply' button at the top (or press Command-R).",
       "Type your answer and click the paper-plane 'Send' button."],
      shortcut="Command-R", app="com.apple.mail")
howto("email_new", "Write a new email", "email",
      ["send email", "new email", "write email", "compose"],
      ["In Mail, press Command-N.",
       "Type the address in 'To', a subject, and your message.",
       "Click the paper-plane 'Send' button."],
      shortcut="Command-N", app="com.apple.mail")

# ── Calls ─────────────────────────────────────────────────────────────────────
howto("facetime_call", "Make a FaceTime video call", "calls",
      ["facetime", "video call", "call my family", "see grandchildren"],
      ["Open FaceTime.",
       "Click 'New FaceTime' and type the person's name, phone number or email.",
       "Click the green 'FaceTime' button."],
      app="com.apple.FaceTime")
howto("zoom_join", "Join a Zoom meeting", "calls",
      ["zoom meeting", "join zoom", "zoom link", "meeting id"],
      ["The easiest way: click the Zoom link in your invitation email.",
       "Or open Zoom, click 'Join', and type the Meeting ID and passcode.",
       "Click 'Join with Computer Audio' so they can hear you."],
      app="us.zoom.xos", settings_topic="camera_access")

# ── Photos ────────────────────────────────────────────────────────────────────
howto("import_iphone_photos", "Copy photos from my iPhone", "photos",
      ["import photos", "photos from phone", "iphone pictures to mac", "transfer photos"],
      ["Connect your iPhone with its cable, unlock it and tap 'Trust' if asked.",
       "In Photos, click your iPhone in the list on the left.",
       "Click 'Import All New Items'."],
      app="com.apple.Photos")
howto("airdrop_receive", "Send a photo from my iPhone with AirDrop", "photos",
      ["airdrop from iphone", "send photo to mac", "share to mac"],
      ["On the iPhone, open the photo and tap the Share button.",
       "Tap 'AirDrop' and choose your Mac.",
       "On the Mac, the photo appears in your Downloads folder."],
      settings_topic="airdrop")
howto("save_web_image", "Save a picture from a website", "photos",
      ["save picture", "download image", "save photo from internet"],
      ["Hold the Control key and click the picture.",
       "Choose 'Save Image to \"Downloads\"'."],
      app=None)
howto("photo_booth", "Take a photo of myself", "photos",
      ["selfie", "take my picture", "photo with camera", "webcam photo"],
      ["Open Photo Booth.",
       "Click the red camera button. After a short countdown the photo is taken."],
      app="com.apple.PhotoBooth")

# ── Web ───────────────────────────────────────────────────────────────────────
howto("web_zoom", "Make a web page bigger", "web",
      ["web page too small", "zoom web page", "bigger website", "text on website small"],
      ["Press Command and the plus key (+) to make the page bigger.",
       "Press Command and minus (-) to make it smaller, or Command-0 to reset."],
      shortcut="Command-Plus", settings_topic="text_size")
howto("bookmark", "Save a website to come back to", "web",
      ["bookmark", "favorite website", "save website"],
      ["While on the page in Safari, press Command-D.",
       "Choose where to save it and click 'Add'."],
      shortcut="Command-D", app="com.apple.Safari")
howto("private_window", "Browse privately", "web",
      ["private browsing", "incognito", "don't save history"],
      ["In Safari, press Shift-Command-N to open a private window.",
       "Pages you visit there aren't saved in your history."],
      shortcut="Shift-Command-N", app="com.apple.Safari")
howto("reopen_tab", "Get back a tab I closed", "web",
      ["closed tab", "reopen tab", "closed page by mistake"],
      ["Press Shift-Command-T in Safari or Chrome to reopen the last closed tab."],
      shortcut="Shift-Command-T")

# ── Printing ──────────────────────────────────────────────────────────────────
howto("print", "Print something", "printing",
      ["print", "print document", "print page"],
      ["Press Command-P.",
       "Check the printer and number of copies.",
       "Click 'Print'."],
      shortcut="Command-P", settings_topic="printers")
howto("save_as_pdf", "Save something as a PDF", "printing",
      ["save as pdf", "make pdf", "pdf"],
      ["Press Command-P as if printing.",
       "Click the 'PDF' menu at the bottom left and choose 'Save as PDF'.",
       "Name it and click 'Save'."],
      shortcut="Command-P")
howto("scan_with_iphone", "Scan a paper document with my iPhone", "printing",
      ["scan document", "scanner", "scan paper"],
      ["In Finder, hold the Control key and click an empty spot on the Desktop.",
       "Choose 'Import from iPhone', then 'Scan Documents'.",
       "Point your iPhone at the paper; the scan appears on your Mac."])

# ── Power and security ────────────────────────────────────────────────────────
howto("restart_shutdown", "Restart or shut down", "power",
      ["restart", "shut down", "turn off computer", "reboot"],
      ["Click the Apple logo at the top left of the screen.",
       "Choose 'Restart…' or 'Shut Down…'."])
howto("lock_screen", "Lock my Mac when I step away", "power",
      ["lock screen", "lock computer", "step away"],
      ["Press Control-Command-Q. Your password is needed to get back in."],
      shortcut="Control-Command-Q", settings_topic="require_password")
howto("sleep", "Put the Mac to sleep", "power",
      ["sleep", "sleep mode", "rest computer"],
      ["Click the Apple logo at the top left and choose 'Sleep'.",
       "On a laptop, you can also just close the lid."])
howto("find_password", "Find a saved password", "power",
      ["forgot password", "saved passwords", "website password"],
      ["Open the Passwords app.",
       "Unlock it with your Mac password or Touch ID.",
       "Search for the website to see its password."],
      app="com.apple.Passwords")

OUT = Path(__file__).resolve().parent.parent / "src/lib/howtoCatalog.json"
OUT.write_text(json.dumps(TOPICS, indent=2, ensure_ascii=False) + "\n")
print(f"{len(TOPICS)} how-to topics → {OUT}")
