import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./DetailPanel.css";

interface Preference {
  id: number;
  rule: string;
  created_at: string;
}

export default function DetailPanel({ onClose }: { onClose: () => void }) {
  const [prefs, setPrefs] = useState<Preference[]>([]);

  async function load() {
    const list = await invoke<Preference[]>("get_all_preferences");
    setPrefs(list);
  }

  useEffect(() => { load(); }, []);

  async function handleDelete(id: number) {
    await invoke("delete_preference", { id });
    load();
  }

  return (
    <div className="detail-panel">
      <div className="detail-header">
        <button className="detail-back" onClick={onClose} title="Back">←</button>
        <span className="detail-title">Memory</span>
      </div>
      <div className="detail-body">
        {prefs.length === 0 ? (
          <p className="detail-empty">
            No preferences saved yet.<br />
            Ask me to remember something during chat — like "always use bullet points".
          </p>
        ) : (
          <ul className="detail-pref-list">
            {prefs.map((p) => (
              <li key={p.id} className="detail-pref-item">
                <span className="detail-pref-rule">{p.rule}</span>
                <button
                  className="detail-pref-delete"
                  onClick={() => handleDelete(p.id)}
                  title="Delete"
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
