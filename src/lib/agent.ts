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
- get_selected_text          — Read the user's currently highlighted/selected text
- get_focused_text           — Read all text in the focused text field
- replace_selected_text      — Replace the user's selected text. Args: {"text": "..."}
- set_focused_text           — Replace all text in the focused field. Args: {"text": "..."}
- read_file                  — Read a file the user dropped. Args: {"path": "..."}
- store_preference           — Save a user preference for future tasks. Args: {"rule": "..."}
- get_all_preferences        — List all stored user preferences

Rules:
- Always read text before editing it.
- Prefer replace_selected_text over set_focused_text when a selection is available.
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
    case "get_selected_text": {
      const text = await invoke<string | null>("get_selected_text");
      return text ?? "(no selection)";
    }
    case "get_focused_text": {
      const text = await invoke<string | null>("get_focused_text");
      return text ?? "(empty field)";
    }
    case "replace_selected_text": {
      await invoke("replace_selected_text", { text: call.args.text ?? "" });
      return "done";
    }
    case "set_focused_text": {
      await invoke("set_focused_text", { text: call.args.text ?? "" });
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
  callbacks: AgentCallbacks
): Promise<string> {
  const { onToken, onStatus, onDroidState } = callbacks;

  const prefs = await invoke<Array<{ rule: string }>>("get_all_preferences").catch(
    () => []
  );
  const systemPrompt = buildSystemPrompt(prefs.map((p) => p.rule));

  // Convert history to the format expected by the Rust command
  const messages = [
    ...history.map((m) => ({ role: m.role === "buddy" ? "model" : "user", content: m.content })),
    { role: "user", content: userMessage },
  ];

  // Agentic loop — up to 5 tool-call rounds
  for (let round = 0; round < 5; round++) {
    onDroidState("thinking");

    let buffer = "";
    let unlisten: UnlistenFn | null = null;

    const tokenDone = new Promise<void>((resolve) => {
      listen<{ text: string; done: boolean }>("llm-token", (event) => {
        if (event.payload.done) {
          unlisten?.();
          resolve();
          return;
        }
        buffer += event.payload.text;
        onToken(event.payload.text);
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
      // Strip any leaked <tool_call> tags from output
      return buffer.replace(/<tool_call>[\s\S]*?<\/tool_call>/g, "").trim();
    }

    // Execute the tool
    onDroidState("working");
    const toolResult = await executeTool(toolCall, onStatus).catch((e) => `Error: ${e}`);
    onStatus("");

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
