import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./TranscriptPanel.css";

type ModelStatus = "checking" | "missing" | "downloading" | "loading" | "ready";

interface TranscriptPanelProps {
  onSendToChat: (text: string) => void;
}

export default function TranscriptPanel({ onSendToChat }: TranscriptPanelProps) {
  const [modelStatus, setModelStatus] = useState<ModelStatus>("checking");
  const [downloadProgress, setDownloadProgress] = useState(0);
  const [downloadError, setDownloadError] = useState("");
  const [segments, setSegments] = useState<string[]>([]);
  const [transcribing, setTranscribing] = useState(false);
  const [startError, setStartError] = useState("");
  const bottomRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to latest transcript
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [segments]);

  // On mount: check model state and wire up event listeners
  useEffect(() => {
    checkModelStatus();

    const unlistenDownload = listen<{ progress: number; done: boolean; error?: string }>(
      "whisper-download-progress",
      async (event) => {
        const { progress, done, error } = event.payload;
        if (error) {
          setDownloadError(error);
          setModelStatus("missing");
        } else if (done) {
          setModelStatus("loading");
          setDownloadError("");
          await loadWhisperModel();
        } else {
          setDownloadProgress(progress);
        }
      }
    );

    const unlistenSegment = listen<{ text: string; done: boolean }>(
      "transcription-segment",
      (event) => {
        if (event.payload.done) {
          setTranscribing(false);
        } else if (event.payload.text) {
          setSegments((s) => [...s, event.payload.text]);
        }
      }
    );

    return () => {
      unlistenDownload.then((fn) => fn());
      unlistenSegment.then((fn) => fn());
    };
  }, []);

  async function checkModelStatus() {
    setModelStatus("checking");
    try {
      const exists = await invoke<boolean>("check_whisper_exists");
      if (!exists) {
        setModelStatus("missing");
        return;
      }
      const loaded = await invoke<boolean>("is_whisper_loaded");
      if (loaded) {
        setModelStatus("ready");
        const isCapturing = await invoke<boolean>("is_transcribing");
        setTranscribing(isCapturing);
      } else {
        setModelStatus("loading");
        await loadWhisperModel();
      }
    } catch {
      setModelStatus("missing");
    }
  }

  async function loadWhisperModel() {
    try {
      await invoke("load_whisper");
      setModelStatus("ready");
    } catch {
      setModelStatus("missing");
    }
  }

  async function handleDownload() {
    setModelStatus("downloading");
    setDownloadProgress(0);
    setDownloadError("");
    try {
      await invoke("start_whisper_download");
    } catch (e) {
      setDownloadError(String(e));
      setModelStatus("missing");
    }
  }

  async function handleStartStop() {
    if (transcribing) {
      await invoke("stop_transcription");
      setTranscribing(false);
    } else {
      setStartError("");
      setSegments([]);
      try {
        await invoke("start_transcription");
        setTranscribing(true);
      } catch (e) {
        setStartError(String(e));
      }
    }
  }

  function handleCopy() {
    navigator.clipboard.writeText(segments.join(" ")).catch(() => null);
  }

  function handleSendToChat() {
    const text = segments.join(" ").trim();
    if (text) onSendToChat(text);
  }

  const fullText = segments.join(" ").trim();

  // ── Render ──────────────────────────────────────────────────────────────────

  if (modelStatus === "checking") {
    return (
      <div className="tp-state">
        <div className="tp-spinner" />
        <span>Checking model…</span>
      </div>
    );
  }

  if (modelStatus === "loading") {
    return (
      <div className="tp-state">
        <div className="tp-spinner" />
        <span>Loading Whisper model…</span>
      </div>
    );
  }

  if (modelStatus === "missing" || modelStatus === "downloading") {
    return (
      <div className="tp-download">
        <div className="tp-download-icon">🎙</div>
        <p className="tp-download-title">Whisper transcription model</p>
        <p className="tp-download-desc">
          ~145 MB · downloaded once · runs entirely on-device
        </p>
        {modelStatus === "downloading" ? (
          <div className="tp-progress-wrap">
            <div className="tp-progress-bar">
              <div className="tp-progress-fill" style={{ width: `${downloadProgress}%` }} />
            </div>
            <span className="tp-progress-label">{Math.round(downloadProgress)}%</span>
          </div>
        ) : (
          <button className="tp-btn tp-btn-primary" onClick={handleDownload}>
            Download model
          </button>
        )}
        {downloadError && <p className="tp-error">{downloadError}</p>}
      </div>
    );
  }

  // modelStatus === "ready"
  return (
    <div className="tp-root">
      <div className="tp-transcript" aria-live="polite">
        {segments.length === 0 ? (
          <p className="tp-empty">
            {transcribing
              ? "Listening… segments will appear every ~5 s."
              : "Press Start to begin capturing meeting audio."}
          </p>
        ) : (
          segments.map((seg, i) => (
            <span key={i} className="tp-segment">
              {seg}{" "}
            </span>
          ))
        )}
        <div ref={bottomRef} />
      </div>

      {startError && <p className="tp-error tp-error-inline">{startError}</p>}

      <div className="tp-controls">
        <button
          className={`tp-btn ${transcribing ? "tp-btn-stop" : "tp-btn-start"}`}
          onClick={handleStartStop}
        >
          {transcribing ? "⏹ Stop" : "⏺ Start"}
        </button>
        {fullText && !transcribing && (
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
          Capturing system audio · grant Screen Recording if prompted
        </p>
      )}
    </div>
  );
}
