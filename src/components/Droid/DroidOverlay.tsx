import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Droid, { DroidState } from "./Droid";
import "./DroidOverlay.css";

export default function DroidOverlay() {
  const [chatOpen, setChatOpen] = useState(false);
  const [dragging, setDragging] = useState(false);

  function handleMouseDown(_e: React.MouseEvent) {
    getCurrentWindow().startDragging().catch(() => null);
  }

  async function handleClick() {
    if (chatOpen) {
      await invoke("hide_chat");
      setChatOpen(false);
    } else {
      await invoke("show_chat");
      setChatOpen(true);
    }
  }

  useEffect(() => {
    const unlistenPromise = getCurrentWindow().onDragDropEvent(async (event) => {
      const p = event.payload;
      if (p.type === "enter") {
        setDragging(true);
      } else if (p.type === "leave") {
        setDragging(false);
      } else if (p.type === "drop") {
        setDragging(false);
        if (p.paths.length === 0) return;
        // Open chat first, then tell it about the files
        await invoke("show_chat");
        setChatOpen(true);
        // Small delay to let the chat window mount its listener
        await new Promise((r) => setTimeout(r, 120));
        await emit("droid-files-dropped", { paths: p.paths });
      }
    });
    return () => { unlistenPromise.then((fn) => fn()); };
  }, []);

  const state: DroidState = "idle";

  return (
    <div
      className={`overlay-root ${dragging ? "overlay-dragging" : ""}`}
      onClick={handleClick}
      onMouseDown={handleMouseDown}
      title={chatOpen ? "Click to close chat" : "Click to open chat"}
    >
      <Droid state={state} size={90} />
      <span className="overlay-hint">⌥ Space</span>
    </div>
  );
}
