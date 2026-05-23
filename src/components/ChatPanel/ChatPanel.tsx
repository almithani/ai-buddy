import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import ReactMarkdown from "react-markdown";
import Droid from "../Droid/Droid";
import { DroidState } from "../Droid/Droid";
import { runAgent, ChatMessage } from "../../lib/agent";
import "./ChatPanel.css";

interface DisplayMessage {
  id: number;
  role: "user" | "buddy";
  text: string;
  streaming?: boolean;
}

interface Resource {
  id: number;
  type: "text" | "file";
  label: string;
  content: string;
}

let nextId = 1;

export default function ChatPanel() {
  const [messages, setMessages] = useState<DisplayMessage[]>([]);
  const [input, setInput] = useState("");
  const [droidState, setDroidState] = useState<DroidState>("idle");
  const [status, setStatus] = useState("");
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState(false);
  const [resources, setResources] = useState<Resource[]>([]);
  const bottomRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const streamingIdRef = useRef<number | null>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Check AX permission on mount. If missing, poll every 2 s until granted,
  // then swap the warning for the normal greeting automatically.
  useEffect(() => {
    let pollInterval: ReturnType<typeof setInterval> | null = null;

    invoke<boolean>("check_accessibility_permission").then((trusted) => {
      setMessages([{
        id: nextId++,
        role: "buddy",
        text: trusted
          ? "Hi! What can I help you with?"
          : "I need **Accessibility permission** to read and edit text in other apps.\n\nGo to **System Settings → Privacy & Security → Accessibility** and add AI Buddy, then come back.",
      }]);

      if (!trusted) {
        pollInterval = setInterval(async () => {
          const nowTrusted = await invoke<boolean>("check_accessibility_permission");
          if (nowTrusted) {
            clearInterval(pollInterval!);
            pollInterval = null;
            setMessages([{ id: nextId++, role: "buddy", text: "Hi! What can I help you with?" }]);
          }
        }, 2000);
      }
    });

    return () => { if (pollInterval) clearInterval(pollInterval); };
  }, []);

  useEffect(() => {
    // "hotkey-triggered" is a bare signal — no payload. We pull the text via
    // get_pending_text() so there is no race with the window becoming visible.
    const unlisten = listen("hotkey-triggered", async () => {
      const { text } = await invoke<{ text: string; debug: string }>("get_pending_text");
      setMessages([{ id: nextId++, role: "buddy", text: "How can I help?" }]);
      setResources(
        text
          ? [{ id: nextId++, type: "text", label: text.slice(0, 50) + (text.length > 50 ? "…" : ""), content: text }]
          : []
      );
      setInput("");
      setBusy(false);
      setTimeout(() => inputRef.current?.focus(), 50);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  async function handleClose() {
    await invoke("hide_chat");
  }

  function handleDragHeaderStart() {
    getCurrentWindow().startDragging().catch(() => null);
  }

  function removeResource(id: number) {
    setResources((r) => r.filter((res) => res.id !== id));
  }

  async function handleSend() {
    const text = input.trim();
    if (!text || busy) return;

    setInput("");
    setBusy(true);

    const capturedResources = resources;
    setResources([]);

    const resourceContext = capturedResources.length > 0
      ? capturedResources
          .map((r) => r.type === "text" ? `Selected text:\n${r.content}` : `File: ${r.label}`)
          .join("\n\n")
      : undefined;

    const userMsg: DisplayMessage = { id: nextId++, role: "user", text };
    setMessages((m) => [...m, userMsg]);

    // Placeholder for the streaming buddy response
    const buddyId = nextId++;
    streamingIdRef.current = buddyId;
    setMessages((m) => [...m, { id: buddyId, role: "buddy", text: "", streaming: true }]);

    // Build history for the agent (exclude the streaming placeholder)
    const history: ChatMessage[] = messages.map((m) => ({
      role: m.role,
      content: m.text,
    }));
    history.push({ role: "user", content: text });

    try {
      const finalText = await runAgent(text, history.slice(0, -1), {
        onToken: (token) => {
          setMessages((m) =>
            m.map((msg) =>
              msg.id === buddyId
                ? { ...msg, text: msg.text + token }
                : msg
            )
          );
        },
        onStatus: (s) => setStatus(s),
        onDroidState: (s) => {
          const map: Record<string, DroidState> = {
            thinking: "thinking",
            working: "working",
            done: "done",
            error: "error",
            idle: "idle",
          };
          setDroidState(map[s] ?? "idle");
        },
      }, resourceContext);

      // Finalise the streaming message with the clean final text
      setMessages((m) =>
        m.map((msg) =>
          msg.id === buddyId ? { ...msg, text: finalText, streaming: false } : msg
        )
      );
    } catch (err) {
      setMessages((m) =>
        m.map((msg) =>
          msg.id === buddyId
            ? { ...msg, text: `Something went wrong: ${err}`, streaming: false }
            : msg
        )
      );
      setDroidState("error");
      setTimeout(() => setDroidState("idle"), 2000);
    } finally {
      setBusy(false);
      setStatus("");
      streamingIdRef.current = null;
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    setDragging(false);
    const files = Array.from(e.dataTransfer.files);
    if (files.length === 0) return;
    const newResources: Resource[] = files.map((f) => ({
      id: nextId++,
      type: "file",
      label: f.name,
      content: f.name,
    }));
    setResources((r) => [...r, ...newResources]);
    inputRef.current?.focus();
  }

  const isThinking = droidState === "thinking" && !messages.find(
    (m) => m.id === streamingIdRef.current && m.text.length > 0
  );

  return (
    <div
      className={`chat-root ${dragging ? "chat-dragging" : ""}`}
      onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
      onDragLeave={() => setDragging(false)}
      onDrop={handleDrop}
    >
      <div className="chat-header" onMouseDown={handleDragHeaderStart}>
        <div className="chat-header-droid">
          <Droid state={droidState} size={32} />
          <div>
            <span className="chat-header-title">AI Buddy</span>
            {status && <span className="chat-status">{status}</span>}
          </div>
        </div>
        <button className="chat-close-btn" onClick={handleClose} title="Close">✕</button>
      </div>

      <div className="chat-messages">
        {messages.map((msg) => (
          <div key={msg.id} className={`chat-msg chat-msg-${msg.role}`}>
            <div className={`chat-msg-bubble ${msg.streaming ? "chat-msg-streaming" : ""}`}>
              {msg.role === "buddy" ? (
                <ReactMarkdown>{msg.text}</ReactMarkdown>
              ) : (
                msg.text
              )}
              {msg.streaming && msg.text.length === 0 && (
                <span className="chat-cursor" />
              )}
            </div>
          </div>
        ))}

        {isThinking && (
          <div className="chat-msg chat-msg-buddy">
            <div className="chat-msg-bubble chat-typing">
              <span /><span /><span />
            </div>
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      {dragging && (
        <div className="chat-drop-overlay">
          <span>Drop files here</span>
        </div>
      )}

      {resources.length > 0 && (
        <div className="chat-resources">
          {resources.map((r) => (
            <div key={r.id} className="chat-resource-chip">
              <span className="chat-resource-chip-icon">{r.type === "text" ? "“" : "□"}</span>
              <span className="chat-resource-chip-label">{r.label}</span>
              <button className="chat-resource-chip-remove" onClick={() => removeResource(r.id)}>✕</button>
            </div>
          ))}
        </div>
      )}

      <div className="chat-input-row">
        <textarea
          ref={inputRef}
          className="chat-input"
          placeholder={busy ? "Working…" : "Type a message… (Enter to send)"}
          value={input}
          rows={1}
          disabled={busy}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
        />
        <button
          className="chat-send-btn"
          onClick={handleSend}
          disabled={!input.trim() || busy}
          title="Send"
        >
          ↑
        </button>
      </div>
    </div>
  );
}
