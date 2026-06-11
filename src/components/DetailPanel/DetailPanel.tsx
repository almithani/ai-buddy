import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MemoryItem, describeMemory } from "../../lib/memory";
import "./DetailPanel.css";

export default function DetailPanel({ onClose }: { onClose: () => void }) {
  const [items, setItems] = useState<MemoryItem[]>([]);

  async function load() {
    const list = await invoke<MemoryItem[]>("get_memory").catch(() => []);
    setItems(list);
  }

  useEffect(() => { load(); }, []);

  async function handleDelete(id: number) {
    await invoke("delete_memory", { id });
    load();
  }

  return (
    <div className="detail-panel">
      <div className="detail-header">
        <button className="detail-back" onClick={onClose} title="Back">←</button>
        <span className="detail-title">Memory</span>
      </div>
      <div className="detail-body">
        {items.length === 0 ? (
          <p className="detail-empty">
            No preferences saved yet.<br />
            Ask me to remember something during chat — like "always use bullet points".
          </p>
        ) : (
          <ul className="detail-pref-list">
            {items.map((m) => (
              <li key={m.id} className="detail-pref-item">
                <span className="detail-pref-rule">{describeMemory(m)}</span>
                <button
                  className="detail-pref-delete"
                  onClick={() => handleDelete(m.id)}
                  title={m.kind === "setting" ? "Reset to default" : "Delete"}
                >
                  ✕
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
