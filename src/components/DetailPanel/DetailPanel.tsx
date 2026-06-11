import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./DetailPanel.css";

interface Preference {
  id: number;
  rule: string;
  created_at: string;
}

interface Setting {
  key: string;
  value: string;
}

/// Friendly display for known setting keys; unknown keys fall back to raw.
function describeSetting(s: Setting): string {
  switch (s.key) {
    case "transcript_dir":
      return `Save transcripts to ${s.value}`;
    case "transcript_include_time":
      return s.value === "false"
        ? "Omit the time from transcript filenames"
        : "Include the time in transcript filenames";
    default:
      return `${s.key} = ${s.value}`;
  }
}

export default function DetailPanel({ onClose }: { onClose: () => void }) {
  const [prefs, setPrefs] = useState<Preference[]>([]);
  const [settings, setSettings] = useState<Setting[]>([]);

  async function load() {
    const list = await invoke<Preference[]>("get_all_preferences");
    setPrefs(list);
    const s = await invoke<Setting[]>("get_all_settings").catch(() => []);
    setSettings(s);
  }

  useEffect(() => { load(); }, []);

  async function handleDelete(id: number) {
    await invoke("delete_preference", { id });
    load();
  }

  async function handleDeleteSetting(key: string) {
    await invoke("delete_setting", { key });
    load();
  }

  return (
    <div className="detail-panel">
      <div className="detail-header">
        <button className="detail-back" onClick={onClose} title="Back">←</button>
        <span className="detail-title">Memory</span>
      </div>
      <div className="detail-body">
        {prefs.length === 0 && settings.length === 0 ? (
          <p className="detail-empty">
            No preferences saved yet.<br />
            Ask me to remember something during chat — like "always use bullet points".
          </p>
        ) : (
          <>
            {prefs.length > 0 && (
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
            {settings.length > 0 && (
              <>
                <p className="detail-section-label">Settings</p>
                <ul className="detail-pref-list">
                  {settings.map((s) => (
                    <li key={s.key} className="detail-pref-item">
                      <span className="detail-pref-rule">{describeSetting(s)}</span>
                      <button
                        className="detail-pref-delete"
                        onClick={() => handleDeleteSetting(s.key)}
                        title="Reset to default"
                      >
                        ✕
                      </button>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </>
        )}
      </div>
    </div>
  );
}
