import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import ReactMarkdown from "react-markdown";
import Droid from "../Droid/Droid";
import { DroidState } from "../Droid/Droid";
import DetailPanel from "../DetailPanel/DetailPanel";
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
  path?: string; // full filesystem path for dropped files
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
  const [showDetail, setShowDetail] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const streamingIdRef = useRef<number | null>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Generates a greeting via the LLM (respects stored preferences).
  // Falls back to a hardcoded string if the model isn't loaded yet.
  async function streamGreeting(prompt: string) {
    const isLoaded = await invoke<boolean>("is_model_loaded").catch(() => false);
    const buddyId = nextId++;
    streamingIdRef.current = buddyId;

    if (!isLoaded) {
      setMessages([{ id: buddyId, role: "buddy", text: "Hi! What can I help you with?", streaming: false }]);
      streamingIdRef.current = null;
      return;
    }

    setMessages([{ id: buddyId, role: "buddy", text: "", streaming: true }]);
    try {
      const finalText = await runAgent(prompt, [], {
        onToken: (token) => {
          setMessages((m) =>
            m.map((msg) => msg.id === buddyId ? { ...msg, text: msg.text + token } : msg)
          );
        },
        onStatus: () => {},
        onDroidState: () => {},
        onReplace: (text) => {
          setMessages((m) =>
            m.map((msg) => msg.id === buddyId ? { ...msg, text } : msg)
          );
        },
      });
      setMessages((m) =>
        m.map((msg) => msg.id === buddyId ? { ...msg, text: finalText, streaming: false } : msg)
      );
    } catch {
      setMessages((m) =>
        m.map((msg) =>
          msg.id === buddyId ? { ...msg, text: "Hi! What can I help you with?", streaming: false } : msg
        )
      );
    } finally {
      streamingIdRef.current = null;
    }
  }

  // Check AX permission on mount. If missing, poll every 2 s until granted,
  // then swap the warning for the normal greeting automatically.
  useEffect(() => {
    let pollInterval: ReturnType<typeof setInterval> | null = null;

    invoke<boolean>("check_accessibility_permission").then((trusted) => {
      if (trusted) {
        streamGreeting("Greet the user. Keep it to one sentence.");
      } else {
        setMessages([{
          id: nextId++,
          role: "buddy",
          text: "I need **Accessibility permission** to read and edit text in other apps.\n\nGo to **System Settings → Privacy & Security → Accessibility** and add AI Buddy, then come back.",
        }]);

        pollInterval = setInterval(async () => {
          const nowTrusted = await invoke<boolean>("check_accessibility_permission");
          if (nowTrusted) {
            clearInterval(pollInterval!);
            pollInterval = null;
            streamGreeting("Greet the user. Keep it to one sentence.");
          }
        }, 2000);
      }
    });

    return () => { if (pollInterval) clearInterval(pollInterval); };
  }, []);

  useEffect(() => {
    const IMAGE_EXTS_SET = new Set(["png","jpg","jpeg","gif","webp","bmp","svg","ico","tiff","heic"]);
    const TEXT_EXTS_SET  = new Set([
      "txt","md","markdown","json","yaml","yml","toml","csv","tsv",
      "js","ts","tsx","jsx","html","css","xml","sh","bash","zsh","py",
      "rs","go","java","c","cpp","h","rb","swift","kt","sql","graphql",
      "env","gitignore","log","tf","lua","r","php","vue","svelte",
    ]);

    const unlisten2 = listen<{ paths: string[] }>("droid-files-dropped", async (event) => {
      const paths = event.payload.paths;
      if (paths.length === 0) return;

      const newResources: Resource[] = await Promise.all(
        paths.map(async (path) => {
          const name = path.split("/").pop() ?? path;
          const ext  = name.split(".").pop()?.toLowerCase() ?? "";

          let content: string;
          if (IMAGE_EXTS_SET.has(ext)) {
            content = "[Image file — image content cannot be sent to the model]";
          } else if (ext === "pdf") {
            content = "[PDF file — PDF text extraction is not yet supported]";
          } else if (TEXT_EXTS_SET.has(ext)) {
            try {
              content = await invoke<string>("read_file", { path });
            } catch {
              content = `[Could not read: ${name}]`;
            }
          } else {
            content = "[Binary file — cannot read content]";
          }

          return { id: nextId++, type: "file" as const, label: name, content, path };
        })
      );

      setResources((r) => [...r, ...newResources]);
      setTimeout(() => inputRef.current?.focus(), 50);
    });

    return () => { unlisten2.then((fn) => fn()); };
  }, []);

  useEffect(() => {
    // "hotkey-triggered" is a bare signal — no payload. We pull the text via
    // get_pending_text() so there is no race with the window becoming visible.
    const unlisten = listen("hotkey-triggered", async () => {
      const { text } = await invoke<{ text: string; debug: string }>("get_pending_text");
      setResources(
        text
          ? [{ id: nextId++, type: "text", label: text.slice(0, 50) + (text.length > 50 ? "…" : ""), content: text }]
          : []
      );
      setInput("");
      setBusy(false);
      await streamGreeting("Greet the user briefly and let them know you're ready to help.");
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
          .map((r) => {
            if (r.type === "text") return `Selected text:\n${r.content}`;
            // Only expose the full path for files whose content was actually read.
            // For images/PDFs/binaries the content is a placeholder starting with "[",
            // and including the path just causes the agent to try read_file on them.
            const contentIsReadable = !r.content.startsWith("[");
            const pathNote = r.path && contentIsReadable ? ` (path: ${r.path})` : "";
            return `File "${r.label}"${pathNote}:\n${r.content}`;
          })
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
        onReplace: (text) => {
          setMessages((m) =>
            m.map((msg) =>
              msg.id === buddyId ? { ...msg, text } : msg
            )
          );
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

  // Tauri intercepts OS file drops before the browser sees them, so
  // e.dataTransfer.files is always empty for Finder drags. We use the
  // native onDragDropEvent instead, which gives us real filesystem paths.
  useEffect(() => {
    const IMAGE_EXTS = new Set(["png","jpg","jpeg","gif","webp","bmp","svg","ico","tiff","heic"]);
    const TEXT_EXTS  = new Set([
      "txt","md","markdown","json","yaml","yml","toml","csv","tsv",
      "js","ts","tsx","jsx","html","css","xml","sh","bash","zsh","py",
      "rs","go","java","c","cpp","h","rb","swift","kt","sql","graphql",
      "env","gitignore","log","tf","lua","r","php","vue","svelte",
    ]);

    const unlistenPromise = getCurrentWindow().onDragDropEvent(async (event) => {
      const p = event.payload;
      if (p.type === "enter") {
        setDragging(true);
      } else if (p.type === "leave") {
        setDragging(false);
      } else if (p.type === "drop") {
        setDragging(false);
        if (p.paths.length === 0) return;

        const newResources: Resource[] = await Promise.all(
          p.paths.map(async (path) => {
            const name = path.split("/").pop() ?? path;
            const ext  = name.split(".").pop()?.toLowerCase() ?? "";

            let content: string;
            if (IMAGE_EXTS.has(ext)) {
              content = "[Image file — image content cannot be sent to the model]";
            } else if (ext === "pdf") {
              content = "[PDF file — PDF text extraction is not yet supported]";
            } else if (TEXT_EXTS.has(ext)) {
              try {
                content = await invoke<string>("read_file", { path });
              } catch {
                content = `[Could not read: ${name}]`;
              }
            } else {
              content = "[Binary file — cannot read content]";
            }

            return { id: nextId++, type: "file" as const, label: name, content, path };
          })
        );

        setResources((r) => [...r, ...newResources]);
        inputRef.current?.focus();
      }
    });

    return () => { unlistenPromise.then((fn) => fn()); };
  }, []);

  function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    setDragging(false);
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
      {showDetail && <DetailPanel onClose={() => setShowDetail(false)} />}

      <div className="chat-header" onMouseDown={handleDragHeaderStart}>
        <div className="chat-header-droid">
          <Droid state={droidState} size={32} />
          <div>
            <span className="chat-header-title">AI Buddy</span>
            {status && <span className="chat-status">{status}</span>}
          </div>
        </div>
        <div style={{ display: "flex", gap: "4px", alignItems: "center" }}>
          <button className="chat-close-btn" onClick={() => setShowDetail((d) => !d)} title="Memory">≡</button>
          <button className="chat-close-btn" onClick={handleClose} title="Close">✕</button>
        </div>
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
