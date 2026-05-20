import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
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

let nextId = 1;

export default function ChatPanel() {
  const [messages, setMessages] = useState<DisplayMessage[]>([
    { id: nextId++, role: "buddy", text: "Hi! What can I help you with?" },
  ]);
  const [input, setInput] = useState("");
  const [droidState, setDroidState] = useState<DroidState>("idle");
  const [status, setStatus] = useState("");
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const streamingIdRef = useRef<number | null>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  async function handleClose() {
    await invoke("hide_chat");
  }

  function handleDragHeaderStart() {
    getCurrentWindow().startDragging().catch(() => null);
  }

  async function handleSend() {
    const text = input.trim();
    if (!text || busy) return;

    setInput("");
    setBusy(true);

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
      });

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
    const names = files.map((f) => f.name).join(", ");
    setInput((v) => (v ? `${v}\n[Dropped: ${names}]` : `[Dropped: ${names}]`));
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
