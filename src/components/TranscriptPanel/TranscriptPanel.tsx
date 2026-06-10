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

function transcriptText(finals: FinalSegment[]): string {
  return toTurns(finals)
    .map((t) => `${speakerLabel(t.source)}: ${t.texts.join(" ")}`)
    .join("\n");
}

export default function TranscriptPanel({ onSendToChat }: TranscriptPanelProps) {
  const [finals, setFinals] = useState<FinalSegment[]>([]);
  const [partials, setPartials] = useState<Partials>({});
  const [transcribing, setTranscribing] = useState(false);
  const [error, setError] = useState("");
  const [permissionDenied, setPermissionDenied] = useState(false);
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

    return () => {
      unlistenSegment.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, []);

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
    navigator.clipboard.writeText(transcriptText(finals)).catch(() => null);
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
              : "Press Start to begin capturing meeting audio."}
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
