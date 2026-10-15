import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { MemoryItem, describeMemory } from "./memory";
import { findSettingsTopic, formatTopicList, searchSettings } from "./settingsGuide";
import { HowtoTopic, findHowto, searchHowtos } from "./howtoGuide";

// ── Types ────────────────────────────────────────────────────────────────────

export interface ChatMessage {
  role: "user" | "buddy";
  content: string;
}

export interface AgentCallbacks {
  onToken: (token: string) => void;
  onStatus: (status: string) => void;
  onDroidState: (state: DroidAgentState) => void;
  onReplace?: (text: string) => void;
}

export type DroidAgentState = "thinking" | "working" | "done" | "error" | "idle";

interface ToolCall {
  name: string;
  args: Record<string, string>;
}

// ── What AI Buddy can do (so it can answer "can you…?" about itself) ─────────
// Features that aren't tools — like meeting transcription, which the user starts
// from the Transcript tab — would otherwise be invisible to the model. Keep this
// short: the whole prompt has to fit a 4096-token context.

const FEATURES = `
What you (AI Buddy) can do — use this to answer questions about yourself, and tell them how to use it:
- Meeting transcription: records both sides of a call or meeting on this Mac, shows it live, then saves notes with a summary to "AI Buddy Transcripts" in Documents. They start and stop it on the Transcript tab. You can change where notes are saved.
- Talk to you: hold Option-Space, speak, and let go to send. Answers are read aloud when they talked (they can ask you to always or never read answers aloud).
- Help with selected text: they highlight text in any app and press Option-Space (or paste it here), then you can summarize it or edit it in place.
- Mac settings: you open the right System Settings page, point at the control and walk them through it.
- Email: you can write a short draft and open it in Gmail or the Mail app ready to send ("email my sister that I'll be late") — they add the address if needed and click Send. Tell you "I use Gmail" or "I use Mail".
- Everyday Mac tasks: screenshots, files, email, video calls, printing and more, with step-by-step help.
- Memory: you remember their preferences ("call me Al", "keep answers short"); they can see and delete these in the Memory panel (the ≡ button).
- Read files they drop onto you (text files).
- Everything runs privately on their Mac.
`.trim();

// ── Tool definitions injected into the system prompt ─────────────────────────

const TOOL_DOCS = `
You have access to the following tools. Use them by outputting a JSON block like this:
<tool_call>{"name": "tool_name", "args": {"key": "value"}}</tool_call>

Available tools:
- read_file                  — Read a file the user dropped. Args: {"path": "..."}
- store_preference           — Save a user preference for future tasks. Args: {"rule": "..."}
- get_memory                 — List everything you remember about the user (preferences and settings)
- set_transcript_settings    — Change where meeting transcripts are auto-saved or their filename format. Args (each optional): {"directory": "~/Desktop", "include_time": "true" or "false"}. Use when the user asks to change where transcripts/meeting minutes are stored, or to include/omit the time in transcript filenames.
- open_settings              — Open the right System Settings page on the user's Mac and get the steps to walk them through it. Args: {"topic": "<id>"}, using an id from "Mac settings that may be relevant" below.
- show_howto                 — Get step-by-step help for an everyday Mac task (screenshots, files, email, calls, printing, …) and open the app it's about. Args: {"topic": "<id>"}, using an id from "How-to guides that may be relevant" below.
- set_email_app              — Remember which email they use, so email help opens the right one. Args: {"app": "gmail" or "mail"}. Use when they say they use Gmail or the Mail app.
- set_voice_settings         — Change whether answers are read out loud. Args: {"speak_replies": "voice" (only when they asked by talking — the default), "always", or "never"}. Use when they say things like "always read your answers out loud" or "stop talking".
- get_mac_info               — Check the Mac's macOS version, battery level, Wi-Fi on/off and sound volume/mute. No args. Use it first when the user asks about these, or when troubleshooting "no sound" / "no internet".

Helping with everyday tasks:
- For "how do I…" questions about using the Mac (taking a screenshot, finding a file, attaching a photo, a frozen app, …), call show_howto with the closest topic. Use open_settings instead when they want to change a setting.
- Give the keyboard shortcut if there is one, plus at most 3 short steps. Don't restate every step from the result.
- You can't write or send email yourself. When they want to write, send, reply to or attach something to an email, call show_howto (email_new, email_reply or email_attach) — never ask for the recipient, subject or message.

Helping with Mac settings (many users are seniors):
- When the user wants to change how their Mac looks, sounds, reads aloud or connects (text size, screen reader, captions, Wi-Fi, Bluetooth, volume, ...), call open_settings with the closest topic. Never make up a topic that isn't listed; if nothing fits, say kindly that you can't help with that setting yet.
- Call open_settings EVERY time the user asks about a setting — even one you helped with earlier in this chat. They may have closed the window, and the setting may have changed.
- Only mention an orange circle or how a setting is right now if the open_settings result for THIS request says so. Never repeat those from earlier replies.
- After it opens, give at most 3 short steps in plain words. No jargon. Put the words they will see on screen in quotes, like 'Text size'.
- If the result includes a caution, tell the user before anything else.
- If the result says how the setting is right now and it is already the way the user wants, tell them it's already set and skip the steps.
- If the result mentions an orange circle, tell the user to look for it — it's easier than describing where to click.

Editing the user's selected text:
- To replace it in place, output the COMPLETE edited text between <replace> and </replace> tags.
- Write real line breaks and keep the original paragraph structure — preserve every blank line. Do NOT collapse paragraphs onto one line, and do NOT wrap the text in JSON or quotes.
- Example:
<replace>
First paragraph.

Second paragraph.
</replace>

Rules:
- The user's selected text is shown in the conversation above — use it as the input for edits.
- If the user asks to summarize attached/selected/pasted text, write the summary directly in your reply as at most 5 short markdown bullet points.
- After editing, confirm briefly in plain language. No markdown.
- If a file attachment contains "[Image file", respond only with: "Image input is not supported yet." Do not attempt to read or describe the image.
- If the user states a general preference ("from now on...", "always..."), call store_preference.
`.trim();

// Placed at the very end of the system prompt: the small local model follows
// the most recent, concrete instructions far better than an adjective ("concise")
// at the top. Replies are also read aloud, where length hurts even more.
const LENGTH_RULES = `
How long to answer (important):
- Keep every reply short: at most 3 short sentences, or at most 3 short steps. (Summaries of their text: up to 5 short bullets.)
- No greeting or warm-up, don't repeat their question, and no closing summary or "let me know if…" line.
- Only give more detail if they ask for it.`;

function buildSystemPrompt(memory: MemoryItem[], settingsQuery: string): string {
  const rules = memory.filter((m) => m.kind === "rule");
  const settings = memory.filter((m) => m.kind === "setting");

  const ruleBlock =
    rules.length > 0
      ? `\nUser preferences (apply these automatically):\n${rules.map((m) => `- ${describeMemory(m)}`).join("\n")}`
      : "";
  const settingBlock =
    settings.length > 0
      ? `\nCurrent settings:\n${settings.map((m) => `- ${describeMemory(m)}`).join("\n")}`
      : "";
  const topics = searchSettings(settingsQuery);
  const topicBlock =
    topics.length > 0
      ? `\nMac settings that may be relevant (topic ids for open_settings):\n${formatTopicList(topics)}`
      : "";

  const howtos = searchHowtos(settingsQuery);
  const howtoBlock =
    howtos.length > 0
      ? `\nHow-to guides that may be relevant (topic ids for show_howto):\n${formatTopicList(howtos)}`
      : "";

  return `You are AI Buddy, a friendly on-screen assistant that helps users with everyday computer tasks. You are concise, helpful, and proactive.\n${FEATURES}\n${TOOL_DOCS}${ruleBlock}${settingBlock}${topicBlock}${howtoBlock}\n${LENGTH_RULES}`;
}

// ── Tool execution ────────────────────────────────────────────────────────────

async function executeTool(
  call: ToolCall,
  onStatus: (s: string) => void
): Promise<string> {
  onStatus(`Using ${call.name}…`);

  switch (call.name) {
    case "replace_selected_text": {
      const editedText = call.args.text ?? "";
      try {
        await invoke("replace_selected_text", { text: editedText });
        return "done";
      } catch {
        // Signal the agent loop to output the text directly without another LLM round
        return `__READ_ONLY__:${editedText}`;
      }
    }
    case "store_preference": {
      const pref = await invoke<{ rule: string }>("store_preference", {
        rule: call.args.rule ?? "",
      });
      return `Saved: "${pref.rule}"`;
    }
    case "get_memory":
    case "get_all_preferences": {
      const items = await invoke<MemoryItem[]>("get_memory");
      if (items.length === 0) return "Nothing remembered yet.";
      return items.map((m) => `- ${describeMemory(m)}`).join("\n");
    }
    case "set_transcript_settings": {
      const updates: string[] = [];
      if (call.args.directory) {
        await invoke("set_setting", { key: "transcript_dir", value: call.args.directory });
        updates.push(`save folder → ${call.args.directory}`);
      }
      if (call.args.include_time !== undefined) {
        const v = String(call.args.include_time) === "false" ? "false" : "true";
        await invoke("set_setting", { key: "transcript_include_time", value: v });
        updates.push(v === "true" ? "filenames include the time" : "filenames omit the time");
      }
      if (updates.length === 0) return "No settings provided — nothing changed.";
      return `Updated transcript settings: ${updates.join("; ")}`;
    }
    case "open_settings": {
      const topic = findSettingsTopic(call.args.topic ?? "");
      if (!topic) {
        const near = searchSettings(call.args.topic ?? "", 5);
        return near.length > 0
          ? `Unknown topic "${call.args.topic ?? ""}". Closest topics:\n${formatTopicList(near)}`
          : `No settings topic matches "${call.args.topic ?? ""}".`;
      }
      let result: { found: boolean; state: string };
      try {
        result = await invoke("open_system_settings", { topic: topic.id });
      } catch (e) {
        return `Could not open System Settings: ${e}`;
      }
      const name = topic.controlLabel ? `'${topic.controlLabel}'` : "the setting";
      const state = result.state ? `\nRight now ${name} is ${result.state}.` : "";
      const ring = result.found ? `\nI've drawn an orange circle around ${name} on screen.` : "";
      const steps = topic.steps.map((s, i) => `${i + 1}. ${s}`).join("\n");
      const caution = topic.caution ? `\nCaution: ${topic.caution}` : "";
      return `Opened System Settings → ${topic.title}.${state}${ring}${caution}\nSteps:\n${steps}`;
    }
    case "show_howto": {
      const topic = findHowto(call.args.topic ?? "");
      if (!topic) {
        const near = searchHowtos(call.args.topic ?? "", 5);
        return near.length > 0
          ? `Unknown topic "${call.args.topic ?? ""}". Closest topics:\n${formatTopicList(near)}`
          : `No how-to guide matches "${call.args.topic ?? ""}".`;
      }
      // Already run this turn (e.g. by the email pre-route) — don't open it twice.
      const earlier = howtosRunThisTurn.get(topic.id);
      if (earlier) return earlier;
      const result = await runHowto(topic);
      howtosRunThisTurn.set(topic.id, result);
      return result;
    }
    case "set_email_app": {
      const raw = String(call.args.app ?? "").toLowerCase();
      // Only save a clear answer — an empty/odd value used to be saved as "mail".
      const app = /gmail|google/.test(raw) ? "gmail" : /\bmail\b|apple/.test(raw) ? "mail" : null;
      if (!app) return "Not saved — ask them in one short sentence: do they use the Mail app or Gmail?";
      await invoke("set_setting", { key: "email_app", value: app });
      const saved = app === "gmail" ? "Saved: they use Gmail." : "Saved: they use the Mail app.";
      // Finish the email guide that was waiting on this answer, rather than
      // relying on the model to remember it (tool results don't carry across turns).
      const pending = pendingEmailHowto ? findHowto(pendingEmailHowto) : undefined;
      const draftRequest = pendingDraftRequest;
      pendingEmailHowto = null;
      pendingDraftRequest = null;
      if (draftRequest) return `${saved}\n${await runEmailDraft(draftRequest, onStatus)}`;
      return pending ? `${saved}\n${await runHowto(pending)}` : saved;
    }
    case "set_voice_settings": {
      const v = String(call.args.speak_replies ?? "").toLowerCase();
      if (!["voice", "always", "never"].includes(v)) return `speak_replies must be "voice", "always" or "never".`;
      await invoke("set_setting", { key: "speak_replies", value: v });
      if (v === "never") await invoke("stop_speaking").catch(() => null);
      return v === "always"
        ? "I'll read every answer out loud."
        : v === "never"
          ? "I won't read answers out loud."
          : "I'll read answers out loud when they ask by talking.";
    }
    case "get_mac_info":
      return await invoke<string>("get_mac_info").catch((e) => `Could not read Mac info: ${e}`);
    case "read_file": {
      const path = call.args.path ?? "";
      if (!path) return "No file path provided.";
      try {
        return await invoke<string>("read_file", { path });
      } catch (e) {
        return `Could not read file: ${e}`;
      }
    }
    default:
      return `Unknown tool: ${call.name}`;
  }
}

/** How-to results already produced in the current turn (cleared per runAgent). */
const howtosRunThisTurn = new Map<string, string>();

/** The email guide for a clear "write/send/reply/attach … email" request, else null. */
function emailGuideFor(message: string): string | null {
  const m = message.toLowerCase();
  if (!/\b(e-?mails?|gmail|mail)\b/.test(m)) return null;
  if (!/\b(send|write|draft|compose|reply|respond|answer|attach|new)\b/.test(m)) return null;
  if (/\b(attach|photo|picture|file|document)\b/.test(m)) return "email_attach";
  if (/\b(reply|respond|answer)\b/.test(m)) return "email_reply";
  return "email_new";
}

/** Email guide waiting for the "Mail or Gmail?" answer (finished by set_email_app). */
let pendingEmailHowto: string | null = null;
/** Request to draft once the "Mail or Gmail?" answer arrives. */
let pendingDraftRequest: string | null = null;

/** The request says what the email should say ("…tells my sister I'll be late"). */
function saysWhatToWrite(message: string): boolean {
  return /\b(tell|tells|telling|say|says|saying|that|about|ask|asks|asking|thank|thanks|thanking|invite|inviting|remind|reminding|let\s+\w+\s+know|wish|wishing|congratulat\w*|apologi[sz]\w*|sorry)\b/i.test(message);
}

/** Writes a short draft with the model and opens it in Gmail or Mail, filled in. */
async function runEmailDraft(request: string, onStatus: (s: string) => void): Promise<string> {
  const emailApp = await emailAppPreference();
  if (emailApp === null) {
    pendingEmailHowto = "email_new";
    pendingDraftRequest = request;
    return "Ask them in one short sentence: do they use the Mail app or Gmail? When they answer, call set_email_app.";
  }
  onStatus("Writing a draft…");
  let draft: { subject: string; body: string };
  try {
    draft = await invoke("draft_email_text", { request });
  } catch {
    // No draft (e.g. model busy) — still open a blank message with the steps.
    const topic = findHowto("email_new");
    return topic ? runHowto(topic) : "Couldn't write a draft.";
  }
  const to = request.match(/[\w.+-]+@[\w-]+(\.[\w-]+)+/)?.[0] ?? "";
  try {
    await invoke("open_email_draft", { app: emailApp, to, subject: draft.subject, body: draft.body });
  } catch (e) {
    return `Couldn't open the draft: ${e}. Here it is so they can copy it:\nSubject: ${draft.subject}\n${draft.body}`;
  }
  const where = emailApp === "gmail" ? "Gmail in their web browser" : "the Mail app";
  const next = to ? "read it, change anything they like, then click Send" : "add the address in 'To', read it, change anything they like, then click Send";
  return `I've written a draft and opened it in ${where}, ready to send.\nSubject: ${draft.subject}\nTell them briefly to ${next}. Don't repeat the whole message.`;
}

/** Opens what a how-to guide is about and returns its steps for the model. */
async function runHowto(topic: HowtoTopic): Promise<string> {
  // Email guides: Mail app or a web service the user said they use.
  let variant: string | undefined;
  if (topic.variants) {
    const emailApp = await emailAppPreference();
    if (emailApp === null) {
      pendingEmailHowto = topic.id;
      return "Ask them in one short sentence: do they use the Mail app or Gmail? When they answer, call set_email_app.";
    }
    if (emailApp !== "mail" && topic.variants[emailApp]) variant = emailApp;
  }
  let opened = "";
  if (topic.app || variant) {
    opened = await invoke<boolean>("open_howto_app", { topic: topic.id, variant })
      .then((ok) => (ok ? (variant ? "\nI've opened Gmail in their web browser." : "\nI've opened the app for them.") : ""))
      .catch((e) => `\nCouldn't open it: ${e}.`);
  }
  const stepList = variant ? topic.variants![variant].steps : topic.steps;
  const steps = stepList.map((s, i) => `${i + 1}. ${s}`).join("\n");
  // Mail's keyboard shortcuts don't apply in Gmail.
  const shortcut = topic.shortcut && !variant ? `\nKeyboard shortcut: ${topic.shortcut}.` : "";
  const related = topic.settingsTopic
    ? `\nRelated setting (use open_settings if they want it): ${topic.settingsTopic}.`
    : "";
  return `How to: ${topic.title}.${opened}${shortcut}\nSteps:\n${steps}${related}`;
}

/** "gmail" | "mail" from the email_app setting; when unset, "mail" if Apple Mail
 *  handles email links on this Mac, otherwise null (ask the user). */
async function emailAppPreference(): Promise<string | null> {
  const saved = await invoke<string | null>("get_setting", { key: "email_app" }).catch(() => null);
  if (saved === "gmail" || saved === "mail") return saved;
  const handler = await invoke<string | null>("default_email_app").catch(() => null);
  return handler === "com.apple.mail" ? "mail" : null;
}

// ── Tool call parsing ─────────────────────────────────────────────────────────

function parseToolCall(text: string): ToolCall | null {
  const match = text.match(/<tool_call>([\s\S]*?)<\/tool_call>/);
  if (!match) return parseFunctionStyleCall(text);
  try {
    return JSON.parse(match[1]) as ToolCall;
  } catch {
    return parseFunctionStyleCall(text);
  }
}

// The smaller model used on 8 GB Macs (Gemma 4 E2B) sometimes writes a tool
// call as code — `print(open_settings(topic="text_size"))` — instead of the
// JSON block. Accept that too, but only for our own tool names.
const CALLABLE_TOOLS = [
  "open_settings", "show_howto", "get_mac_info", "store_preference", "get_memory",
  "set_transcript_settings", "set_voice_settings", "set_email_app", "read_file",
];
const FUNCTION_CALL = new RegExp(`\\b(${CALLABLE_TOOLS.join("|")})\\s*\\(([^()]*)\\)`);

function parseFunctionStyleCall(text: string): ToolCall | null {
  const m = text.match(FUNCTION_CALL);
  if (!m) return null;
  const args: Record<string, string> = {};
  for (const a of m[2].matchAll(/(\w+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([\w.\/~-]+))/g)) {
    args[a[1]] = a[2] ?? a[3] ?? a[4];
  }
  return { name: m[1], args };
}

// In-place edit: the replacement text is raw (not JSON) so line breaks and
// paragraphs survive verbatim. Strips one leading/trailing newline the model
// tends to add for readability, keeping all internal structure.
function parseEditBlock(text: string): ToolCall | null {
  const match = text.match(/<replace>([\s\S]*?)<\/replace>/);
  if (!match) return null;
  const inner = match[1].replace(/^\r?\n/, "").replace(/\r?\n$/, "");
  return { name: "replace_selected_text", args: { text: inner } };
}

// ── Main agent loop ───────────────────────────────────────────────────────────

// Phrases that only make sense right after open_settings has run.
const SETTINGS_CLAIM = /orange circle|circled|system settings|settings page|already (set|on|off|correct)|slider|switch (it )?on/i;

export async function runAgent(
  userMessage: string,
  history: ChatMessage[],
  callbacks: AgentCallbacks,
  resourceContext?: string
): Promise<string> {
  const { onToken, onStatus, onDroidState, onReplace } = callbacks;

  const memory = await invoke<MemoryItem[]>("get_memory").catch(() => []);
  // Include the previous user turn so follow-ups ("yes, do that") still match.
  const lastUser = [...history].reverse().find((m) => m.role === "user")?.content ?? "";
  const settingsQuery = `${userMessage} ${lastUser}`;
  const systemPrompt = buildSystemPrompt(memory, settingsQuery);
  const settingsRelevant = searchSettings(settingsQuery, 1).length > 0;
  let settingsOpened = false;
  // For settings-like requests, don't stream the first round: when the model
  // behaves it's only a (hidden) tool call anyway, and when it answers in prose
  // instead, the nudge below replaces it — without it flashing on screen first.
  const holdFirstRound = !resourceContext && searchSettings(userMessage, 1).length > 0;
  let nudged = false;

  const finalUserMessage = resourceContext
    ? `${resourceContext}\n\n${userMessage}`
    : userMessage;

  // Convert history to the format expected by the Rust command
  const messages = [
    ...history.map((m) => ({ role: m.role === "buddy" ? "model" : "user", content: m.content })),
    { role: "user", content: finalUserMessage },
  ];

  howtosRunThisTurn.clear();

  // Clear "write/send/reply/attach … email" requests open the email guide
  // deterministically (Gmail or Mail) — the small model often answered from its
  // own knowledge ("click Compose…") without calling show_howto, so nothing
  // opened. Skipped when text is attached (they're working with that text).
  const emailTopic = resourceContext ? null : emailGuideFor(userMessage);
  if (emailTopic && !pendingEmailHowto) {
    // "…tells my sister I'll be late" → write the draft and open it filled in.
    const draft = emailTopic === "email_new" && saysWhatToWrite(userMessage);
    const call: ToolCall = draft
      ? { name: "draft_email", args: { request: userMessage } }
      : { name: "show_howto", args: { topic: emailTopic } };
    const result = draft ? await runEmailDraft(userMessage, onStatus) : await executeTool(call, onStatus);
    onStatus("");
    messages.push({ role: "model", content: `<tool_call>${JSON.stringify(call)}</tool_call>` });
    messages.push({ role: "user", content: `<tool_result>${result}</tool_result>` });
  }

  // Answer to our own "Mail app or Gmail?" question: record it and finish the
  // waiting email guide here, so the model only has to phrase the steps (the
  // small model tended to start drafting the email instead). Anything else
  // drops the question.
  if (pendingEmailHowto) {
    const answer = /gmail/i.test(userMessage) ? "gmail" : /\bmail\b/i.test(userMessage) ? "mail" : null;
    if (answer) {
      const call = { name: "set_email_app", args: { app: answer } };
      const result = await executeTool(call, onStatus);
      onStatus("");
      messages.push({ role: "model", content: `<tool_call>${JSON.stringify(call)}</tool_call>` });
      messages.push({ role: "user", content: `<tool_result>${result}</tool_result>` });
    } else {
      pendingEmailHowto = null;
    }
  }

  // Strip <tool_call> and <replace> blocks (complete or in-progress) from the
  // display text. Also holds back any trailing chars that could be the start of
  // one of those tags so partial tags never flash on screen.
  const HIDDEN_TAGS = ["<tool_call>", "<replace>"] as const;
  function visibleText(buf: string): string {
    let text = buf
      .replace(/<tool_call>[\s\S]*?<\/tool_call>/g, "")
      .replace(/<replace>[\s\S]*?<\/replace>/g, "");
    // An unclosed block is still streaming — hide from its opening tag onward.
    for (const tag of HIDDEN_TAGS) {
      const idx = text.indexOf(tag);
      if (idx >= 0) text = text.slice(0, idx);
    }
    // Hold back a trailing partial opening tag (e.g. "<rep").
    for (const tag of HIDDEN_TAGS) {
      for (let len = Math.min(tag.length - 1, text.length); len > 0; len--) {
        if (tag.startsWith(text.slice(-len))) {
          text = text.slice(0, -len);
          break;
        }
      }
    }
    return text;
  }

  // Agentic loop — up to 5 tool-call rounds
  for (let round = 0; round < 5; round++) {
    onDroidState("thinking");

    let buffer = "";
    let emitted = 0;
    let unlisten: UnlistenFn | null = null;

    const tokenDone = new Promise<void>((resolve) => {
      listen<{ text: string; done: boolean }>("llm-token", (event) => {
        if (event.payload.done) {
          unlisten?.();
          resolve();
          return;
        }
        buffer += event.payload.text;
        if (round === 0 && holdFirstRound) return;
        const display = visibleText(buffer);
        if (display.length > emitted) {
          onToken(display.slice(emitted));
          emitted = display.length;
        }
      }).then((fn) => {
        unlisten = fn;
      });
    });

    await invoke("generate_response", {
      messages,
      systemPrompt,
      maxTokens: 512,
    });
    await tokenDone;

    // Prefer the raw-text edit block (preserves line breaks) over a JSON tool.
    const toolCall = parseEditBlock(buffer) ?? parseToolCall(buffer);

    // The small model sometimes copies an earlier reply ("look for the orange
    // circle…") instead of calling open_settings, so the page never opens.
    // If a reply talks about settings actions without the tool having run this
    // turn, send it back once to actually open the page.
    if (!toolCall && settingsRelevant && !settingsOpened && !nudged && SETTINGS_CLAIM.test(buffer)) {
      nudged = true;
      onReplace?.("");
      messages.push({ role: "model", content: buffer });
      messages.push({
        role: "user",
        content:
          "<tool_result>You have not opened System Settings for this request. Call open_settings now with the best topic id, then answer using only its result.</tool_result>",
      });
      continue;
    }

    if (!toolCall) {
      // No tool call — final answer
      onDroidState("done");
      setTimeout(() => onDroidState("idle"), 1500);
      return visibleText(buffer).trim();
    }

    // Execute the tool
    if (toolCall.name === "open_settings") settingsOpened = true;
    onDroidState("working");
    const toolResult = await executeTool(toolCall, onStatus).catch((e) => `Error: ${e}`);
    onStatus("");

    // replace_selected_text short-circuits — no second LLM round either way
    if (toolResult === "done") {
      onReplace?.("");
      onDroidState("done");
      setTimeout(() => onDroidState("idle"), 1500);
      return "Done — text updated.";
    }
    if (toolResult.startsWith("__READ_ONLY__:")) {
      const editedText = toolResult.slice("__READ_ONLY__:".length);
      onReplace?.("");
      onDroidState("done");
      setTimeout(() => onDroidState("idle"), 1500);
      return `The field is read-only so I couldn't edit in place. Here's the updated version — you can copy it:\n\n${editedText}`;
    }

    // Clear the streaming bubble before the next round streams the final reply
    onReplace?.("");

    // Add the round to message history and continue
    messages.push({ role: "model", content: buffer });
    messages.push({
      role: "user",
      content: `<tool_result>${toolResult}</tool_result>`,
    });
  }

  onDroidState("error");
  setTimeout(() => onDroidState("idle"), 2000);
  return "I got stuck in a loop. Please try rephrasing your request.";
}
