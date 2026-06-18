import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./TranscriptPanel.css";

type Source = "me" | "them";

interface FinalSegment {
  id: number;
  source: Source;
  text: string;
  ts: number;
}

type Partials = Partial<Record<Source, { text: string; ts: number }>>;

interface Turn {
  source: Source;
  ts: number;
  texts: string[];
}

interface TranscriptPanelProps {
  onSendToChat: (text: string) => void;
}

function toTurns(finals: FinalSegment[]): Turn[] {
  const turns: Turn[] = [];
  for (const seg of finals) {
    const last = turns[turns.length - 1];
    if (last && last.source === seg.source) {
      last.texts.push(seg.text);
    } else {
      turns.push({ source: seg.source, ts: seg.ts, texts: [seg.text] });
    }
  }
  return turns;
}

function speakerLabel(source: Source): string {
  return source === "me" ? "Me" : "Them";
}

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function basename(path: string): string {
  return path.split("/").pop() ?? path;
}

function transcriptText(finals: FinalSegment[]): string {
  return toTurns(finals)
    .map((t) => `${speakerLabel(t.source)}: ${t.texts.join(" ")}`)
    .join("\n");
}

/// Same speaker-turn format as the auto-saved .md file.
function transcriptMarkdown(finals: FinalSegment[]): string {
  return toTurns(finals)
    .map((t) => `**${speakerLabel(t.source)}** (${formatTime(t.ts)}): ${t.texts.join(" ")}`)
    .join("\n\n");
}

export default function TranscriptPanel({ onSendToChat }: TranscriptPanelProps) {
  const [finals, setFinals] = useState<FinalSegment[]>([]);
  const [partials, setPartials] = useState<Partials>({});
  const [transcribing, setTranscribing] = useState(false);
  const [error, setError] = useState("");
  const [permissionDenied, setPermissionDenied] = useState(false);
  const [livePath, setLivePath] = useState<string | null>(null);
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [saveFailed, setSaveFailed] = useState(false);
  const [stage, setStage] = useState<string | null>(null);
  const [diarStatus, setDiarStatus] = useState<"unknown" | "missing" | "installed" | "downloading">("unknown");
  const [diarProgress, setDiarProgress] = useState(0);
  const bottomRef = useRef<HTMLDivElement>(null);
  const nextId = useRef(1);
  const partialsRef = useRef<Partials>({});
  partialsRef.current = partials;

  // Auto-scroll to latest transcript
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [finals, partials]);

  useEffect(() => {
    invoke<boolean>("is_transcribing")
      .then(setTranscribing)
      .catch(() => null);

    // Restore the transcript from the backend — panel state is destroyed on
    // tab switches, but the backend keeps the current/last session's segments.
    invoke<{ source: Source; text: string; tsMs: number }[]>("get_transcript")
      .then((stored) => {
        if (stored.length === 0) return;
        setFinals((existing) =>
          existing.length >= stored.length
            ? existing
            : stored.map((s) => ({
                id: nextId.current++,
                source: s.source,
                text: s.text,
                ts: s.tsMs,
              }))
        );
      })
      .catch(() => null);

    const unlistenSegment = listen<{ source: Source; text: string; isFinal: boolean }>(
      "transcription-segment",
      (event) => {
        const { source, text, isFinal } = event.payload;
        if (isFinal) {
          const ts = partialsRef.current[source]?.ts ?? Date.now();
          setPartials((p) => {
            const next = { ...p };
            delete next[source];
            return next;
          });
          if (text.trim()) {
            setFinals((f) => [...f, { id: nextId.current++, source, text, ts }]);
          }
        } else {
          setPartials((p) => ({
            ...p,
            [source]: { text, ts: p[source]?.ts ?? Date.now() },
          }));
        }
      }
    );

    const unlistenError = listen<string>("transcription-error", (event) => {
      setError(event.payload);
      setTranscribing(false);
    });

    // Status bar: which file we're recording to / last saved to.
    invoke<{ live: string | null; saved: string | null }>("get_transcript_files")
      .then((f) => {
        setLivePath(f.live);
        setSavedPath(f.saved);
      })
      .catch(() => null);
    const unlistenStarted = listen<string>("transcription-started", (event) => {
      setLivePath(event.payload || null);
      setSaveFailed(false);
      setStage(null);
    });
    const unlistenSaved = listen<string>("transcript-saved", (event) => {
      setSavedPath(event.payload);
      setLivePath(null);
      setStage(null);
    });
    const unlistenSaveFailed = listen<string>("transcript-save-failed", () => {
      // The live file still exists on disk — keep linking to it for recovery.
      setSaveFailed(true);
      setStage(null);
    });
    const unlistenDiscarded = listen("transcript-discarded", () => {
      setLivePath(null); // empty session: live file was deleted
      setStage(null);
    });
    // Per-stage progress during the post-Stop save (diarization, summary…).
    const unlistenProgress = listen<string>("transcript-progress", (event) => {
      setStage(event.payload);
    });

    // Speaker-diarization model availability (offer download if missing).
    invoke<string>("diarization_models_status")
      .then((s) => setDiarStatus(s === "installed" ? "installed" : "missing"))
      .catch(() => setDiarStatus("installed")); // hide the note if the check fails
    const unlistenDiar = listen<{ progress: number; done: boolean; error?: string }>(
      "diarization-download-progress",
      (event) => {
        const { progress, done, error } = event.payload;
        if (error) {
          setDiarStatus("missing");
        } else if (done) {
          setDiarStatus("installed");
        } else {
          setDiarProgress(progress);
        }
      }
    );

    return () => {
      unlistenSegment.then((fn) => fn());
      unlistenError.then((fn) => fn());
      unlistenStarted.then((fn) => fn());
      unlistenSaved.then((fn) => fn());
      unlistenSaveFailed.then((fn) => fn());
      unlistenDiscarded.then((fn) => fn());
      unlistenProgress.then((fn) => fn());
      unlistenDiar.then((fn) => fn());
    };
  }, []);

  function handleDiarDownload() {
    setDiarStatus("downloading");
    setDiarProgress(0);
    invoke("install_diarization_models").catch(() => setDiarStatus("missing"));
  }

  async function handleStartStop() {
    if (transcribing) {
      await invoke("stop_transcription").catch(() => null);
      setTranscribing(false);
      setPartials({});
      return;
    }

    setError("");
    setPermissionDenied(false);
    setFinals([]);
    setPartials({});

    try {
      // macOS 26+: SpeechAnalyzer needs its on-device model installed once
      // (system-wide, often already present via Dictation).
      const assets = await invoke<string>("speech_assets_status");
      if (assets === "download-required") {
        setError("Downloading the on-device speech model — transcription starts automatically when it finishes…");
        await invoke("install_speech_assets");
        setError("");
      }

      let status = await invoke<string>("transcription_auth_status");
      if (status === "notDetermined") {
        status = await invoke<string>("request_transcription_permission");
      }
      if (status !== "authorized") {
        setPermissionDenied(true);
        return;
      }
      await invoke("start_transcription");
      setTranscribing(true);
    } catch (e) {
      setError(String(e));
    }
  }

  function handleCopy() {
    navigator.clipboard.writeText(transcriptMarkdown(finals)).catch(() => null);
  }

  function handleSendToChat() {
    const text = transcriptText(finals);
    if (text) onSendToChat(text);
  }

  function handleOpenSettings() {
    invoke("open_speech_settings").catch(() => null);
  }

  const turns = toTurns(finals);
  const livePartials = (["me", "them"] as Source[])
    .filter((s) => partials[s]?.text)
    .map((s) => ({ source: s, ...partials[s]! }));
  const hasContent = turns.length > 0 || livePartials.length > 0;

  return (
    <div className="tp-root">
      <div className="tp-transcript" aria-live="polite">
        {!hasContent ? (
          <p className="tp-empty">
            {transcribing
              ? "Listening… your words appear as “Me”, other participants as “Them”."
              : "Press Start to begin capturing audio privately. Works on every meeting platform — or even by yourself!"}
          </p>
        ) : (
          <>
            {turns.map((turn, i) => (
              <div key={i} className="tp-turn">
                <div className="tp-turn-head">
                  <span className={`tp-speaker tp-speaker-${turn.source}`}>
                    {speakerLabel(turn.source)}
                  </span>
                  <span className="tp-time">{formatTime(turn.ts)}</span>
                </div>
                <p className="tp-turn-text">{turn.texts.join(" ")}</p>
              </div>
            ))}
            {livePartials.map((p) => (
              <div key={`partial-${p.source}`} className="tp-turn tp-partial">
                <div className="tp-turn-head">
                  <span className={`tp-speaker tp-speaker-${p.source}`}>
                    {speakerLabel(p.source)}
                  </span>
                  <span className="tp-time">{formatTime(p.ts)}</span>
                </div>
                <p className="tp-turn-text">{p.text}</p>
              </div>
            ))}
          </>
        )}
        <div ref={bottomRef} />
      </div>

      {error && <p className="tp-error tp-error-inline">{error}</p>}

      {!transcribing && diarStatus === "missing" && (
        <div className="tp-diar-note">
          <span>🗣 Identify individual speakers in meetings</span>
          <button className="tp-btn tp-btn-ghost" onClick={handleDiarDownload}>
            Download model (~36 MB)
          </button>
        </div>
      )}
      {diarStatus === "downloading" && (
        <div className="tp-diar-note">
          <span>Downloading speaker model… {Math.round(diarProgress)}%</span>
        </div>
      )}

      {livePath ? (
        <div className="tp-statusbar">
          {transcribing && <span className="tp-status-dot" />}
          {!transcribing && !saveFailed && <span className="tp-status-spinner" />}
          <a
            className="tp-status-link"
            title={livePath}
            onClick={() => invoke("reveal_in_finder", { path: livePath }).catch(() => null)}
          >
            {basename(livePath)}
          </a>
          <span className="tp-status-state tp-status-right">
            {transcribing
              ? livePartials.length > 0
                ? "unsaved changes"
                : "saved"
              : saveFailed
                ? "save failed — file kept"
                : stage ?? "finalizing…"}
          </span>
        </div>
      ) : savedPath ? (
        <div className="tp-statusbar">
          <span className="tp-status-state">Saved to</span>
          <a
            className="tp-status-link"
            title={savedPath}
            onClick={() => invoke("reveal_in_finder", { path: savedPath }).catch(() => null)}
          >
            {basename(savedPath)}
          </a>
        </div>
      ) : null}

      {permissionDenied && (
        <div className="tp-permission">
          <p className="tp-error">
            Speech Recognition permission is required for transcription.
          </p>
          <button className="tp-btn tp-btn-ghost" onClick={handleOpenSettings}>
            Open System Settings
          </button>
        </div>
      )}

      <div className="tp-controls">
        <button
          className={`tp-btn ${transcribing ? "tp-btn-stop" : "tp-btn-start"}`}
          onClick={handleStartStop}
        >
          {transcribing ? "⏹ Stop" : "⏺ Start"}
        </button>
        {turns.length > 0 && !transcribing && (
          <>
            <button className="tp-btn tp-btn-ghost" onClick={handleCopy} title="Copy transcript">
              Copy
            </button>
            <button className="tp-btn tp-btn-ghost" onClick={handleSendToChat} title="Add to chat">
              → Chat
            </button>
          </>
        )}
      </div>

      {transcribing && (
        <p className="tp-hint">
          Transcribing on-device · grant Screen Recording if prompted
        </p>
      )}
    </div>
  );
}
