import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Droid, { DroidState } from "./Droid";
import "./DroidOverlay.css";

export default function DroidOverlay() {
  const [chatOpen, setChatOpen] = useState(false);

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

  // The droid state will be driven by the agent in milestone 2.
  // For now, idle.
  const state: DroidState = "idle";

  return (
    <div
      className="overlay-root"
      onClick={handleClick}
      onMouseDown={handleMouseDown}
      title={chatOpen ? "Click to close chat" : "Click to open chat"}
    >
      <Droid state={state} size={90} />
    </div>
  );
}
