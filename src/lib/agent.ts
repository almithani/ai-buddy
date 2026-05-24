import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

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
- get_all_preferences        — List all stored user preferences

Rules:
- The user's selected text is shown in the conversation above — use it as the input for edits.
- After editing, confirm briefly in plain language. No markdown.
- If the user states a general preference ("from now on...", "always..."), call store_preference.
`.trim();

function buildSystemPrompt(preferences: string[]): string {
  const prefBlock =
    preferences.length > 0
      ? `\nUser preferences (apply these automatically):\n${preferences.map((p) => `- ${p}`).join("\n")}`
      : "";

  return `You are AI Buddy, a friendly on-screen assistant that helps users with everyday computer tasks. You are concise, helpful, and proactive. ${TOOL_DOCS}${prefBlock}`;
}

// ── Tool execution ────────────────────────────────────────────────────────────

async function executeTool(
  call: ToolCall,
  onStatus: (s: string) => void
): Promise<string> {
  onStatus(`Using ${call.name}…`);

  switch (call.name) {
    case "replace_selected_text": {
      await invoke("replace_selected_text", { text: call.args.text ?? "" });
      return "done";
    }
    case "store_preference": {
      const pref = await invoke<{ rule: string }>("store_preference", {
        rule: call.args.rule ?? "",
      });
      return `Saved: "${pref.rule}"`;
    }
    case "get_all_preferences": {
      const prefs = await invoke<Array<{ rule: string }>>("get_all_preferences");
      if (prefs.length === 0) return "No preferences saved yet.";
      return prefs.map((p) => `- ${p.rule}`).join("\n");
    }
    case "read_file": {
      // Basic text read — will be extended in later milestone
      return `(file reading not yet implemented — milestone 3)`;
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

  const prefs = await invoke<Array<{ rule: string }>>("get_all_preferences").catch(
    () => []
  );
  const systemPrompt = buildSystemPrompt(prefs.map((p) => p.rule));

  const finalUserMessage = resourceContext
    ? `${resourceContext}\n\n${userMessage}`
    : userMessage;

  // Convert history to the format expected by the Rust command
  const messages = [
    ...history.map((m) => ({ role: m.role === "buddy" ? "model" : "user", content: m.content })),
    { role: "user", content: finalUserMessage },
  ];

  // Strip <tool_call> blocks (complete or in-progress) from the display text.
  function visibleText(buf: string): string {
    let text = buf.replace(/<tool_call>[\s\S]*?<\/tool_call>/g, "");
    const idx = text.indexOf("<tool_call>");
    if (idx >= 0) text = text.slice(0, idx);
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
