import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { MemoryItem, describeMemory } from "./memory";

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

// ── Tool definitions injected into the system prompt ─────────────────────────

const TOOL_DOCS = `
You have access to the following tools. Use them by outputting a JSON block like this:
<tool_call>{"name": "tool_name", "args": {"key": "value"}}</tool_call>

Available tools:
- replace_selected_text      — Replace the user's selected text. Args: {"text": "..."}
- read_file                  — Read a file the user dropped. Args: {"path": "..."}
- store_preference           — Save a user preference for future tasks. Args: {"rule": "..."}
- get_memory                 — List everything you remember about the user (preferences and settings)
- set_transcript_settings    — Change where meeting transcripts are auto-saved or their filename format. Args (each optional): {"directory": "~/Desktop", "include_time": "true" or "false"}. Use when the user asks to change where transcripts/meeting minutes are stored, or to include/omit the time in transcript filenames.

Rules:
- The user's selected text is shown in the conversation above — use it as the input for edits.
- If the user asks to summarize attached/selected/pasted text, write the summary directly in your reply as concise markdown bullet points. Do not use a tool for summarizing.
- After editing, confirm briefly in plain language. No markdown.
- If replace_selected_text returns an error, the field is read-only. Output the edited text directly in your reply instead, and tell the user they can copy it.
- If a file attachment contains "[Image file", respond only with: "Image input is not supported yet." Do not attempt to read or describe the image.
- If the user states a general preference ("from now on...", "always..."), call store_preference.
`.trim();

function buildSystemPrompt(memory: MemoryItem[]): string {
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

  return `You are AI Buddy, a friendly on-screen assistant that helps users with everyday computer tasks. You are concise, helpful, and proactive. ${TOOL_DOCS}${ruleBlock}${settingBlock}`;
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

// ── Tool call parsing ─────────────────────────────────────────────────────────

function parseToolCall(text: string): ToolCall | null {
  const match = text.match(/<tool_call>([\s\S]*?)<\/tool_call>/);
  if (!match) return null;
  try {
    return JSON.parse(match[1]) as ToolCall;
  } catch {
    return null;
  }
}

// ── Main agent loop ───────────────────────────────────────────────────────────

export async function runAgent(
  userMessage: string,
  history: ChatMessage[],
  callbacks: AgentCallbacks,
  resourceContext?: string
): Promise<string> {
  const { onToken, onStatus, onDroidState, onReplace } = callbacks;

  const memory = await invoke<MemoryItem[]>("get_memory").catch(() => []);
  const systemPrompt = buildSystemPrompt(memory);

  const finalUserMessage = resourceContext
    ? `${resourceContext}\n\n${userMessage}`
    : userMessage;

  // Convert history to the format expected by the Rust command
  const messages = [
    ...history.map((m) => ({ role: m.role === "buddy" ? "model" : "user", content: m.content })),
    { role: "user", content: finalUserMessage },
  ];

  // Strip <tool_call> blocks (complete or in-progress) from the display text.
  // Also holds back any trailing chars that could be the start of <tool_call>
  // so partial tags never flash on screen.
  function visibleText(buf: string): string {
    let text = buf.replace(/<tool_call>[\s\S]*?<\/tool_call>/g, "");
    const idx = text.indexOf("<tool_call>");
    if (idx >= 0) text = text.slice(0, idx);
    const tag = "<tool_call>";
    for (let len = Math.min(tag.length - 1, text.length); len > 0; len--) {
      if (tag.startsWith(text.slice(-len))) {
        text = text.slice(0, -len);
        break;
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

    const toolCall = parseToolCall(buffer);

    if (!toolCall) {
      // No tool call — final answer
      onDroidState("done");
      setTimeout(() => onDroidState("idle"), 1500);
      return visibleText(buffer).trim();
    }

    // Execute the tool
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
