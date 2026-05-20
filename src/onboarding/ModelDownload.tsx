import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Droid from "../components/Droid/Droid";

interface DownloadProgress {
  progress: number;
  downloaded_bytes: number;
  total_bytes: number;
  done: boolean;
  error: string | null;
}

interface ModelDownloadProps {
  onNext: () => void;
}

const TOTAL_GB = 5.0;

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 GB";
  return (bytes / 1e9).toFixed(1) + " GB";
}

export default function ModelDownload({ onNext }: ModelDownloadProps) {
  const [progress, setProgress] = useState(0);
  const [downloaded, setDownloaded] = useState(0);
  const [total, setTotal] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [cancelled, setCancelled] = useState(false);
  const doneRef = useRef(false);

  useEffect(() => {
    // Check if already downloaded
    invoke<boolean>("check_model_exists").then((exists) => {
      if (exists) {
        setProgress(100);
        doneRef.current = true;
        setTimeout(onNext, 600);
      }
    });
  }, [onNext]);

  useEffect(() => {
    if (cancelled || doneRef.current) return;

    let unlisten: (() => void) | null = null;

    const setup = async () => {
      unlisten = await listen<DownloadProgress>("model-download-progress", (event) => {
        const p = event.payload;
        setProgress(p.progress);
        setDownloaded(p.downloaded_bytes);
        setTotal(p.total_bytes);

        if (p.error) {
          setError(p.error);
          return;
        }
        if (p.done && !doneRef.current) {
          doneRef.current = true;
          setTimeout(onNext, 700);
        }
      });

      try {
        await invoke("start_model_download");
      } catch (e) {
        setError(String(e));
      }
    };

    setup();
    return () => unlisten?.();
  }, [cancelled, onNext]);

  async function handleCancel() {
    setCancelled(true);
    await invoke("cancel_model_download").catch(() => null);
  }

  function handleRetry() {
    doneRef.current = false;
    setError(null);
    setCancelled(false);
    setProgress(0);
  }

  const droidState = progress >= 100 ? "done" : error ? "error" : "thinking";
  const downloadedLabel = total > 0 ? formatBytes(downloaded) : `${(progress / 100 * TOTAL_GB).toFixed(1)} GB`;
  const totalLabel = total > 0 ? formatBytes(total) : `${TOTAL_GB} GB`;

  return (
    <div className="onboarding-root ob-screen-enter">
      <div className="ob-droid-stage">
        <Droid state={droidState} taskIcon={progress < 100 && !error ? "thinking" : null} size={130} />
        <div className="ob-bubble">
          {error ? (
            <>Something went wrong with the download.</>
          ) : progress >= 100 ? (
            <>Got it — my brain is ready! 🎉</>
          ) : (
            <>
              Downloading my brain — about <strong>{TOTAL_GB} GB</strong>,
              happens once.{" "}
              <span style={{ color: "var(--text-muted)", fontSize: 12 }}>
                WiFi recommended.
              </span>
            </>
          )}
        </div>
      </div>

      <div className="ob-content">
        {error ? (
          <div style={{ textAlign: "center", color: "var(--text-muted)", fontSize: 13 }}>
            <p style={{ color: "var(--error)", marginBottom: 8 }}>{error}</p>
            <button className="ob-btn-primary" onClick={handleRetry}>
              Try again
            </button>
          </div>
        ) : cancelled ? (
          <div style={{ textAlign: "center", color: "var(--text-muted)", fontSize: 13 }}>
            <p>Download cancelled.</p>
            <button className="ob-btn-primary" style={{ marginTop: 12 }} onClick={handleRetry}>
              Try again
            </button>
          </div>
        ) : (
          <>
            <div className="ob-progress-wrap">
              <div className="ob-progress-bar-track">
                <div className="ob-progress-bar-fill" style={{ width: `${progress}%` }} />
              </div>
              <div className="ob-progress-label">
                <span>
                  {progress >= 100
                    ? "Download complete"
                    : `Gemma 4 4B · ${downloadedLabel} of ${totalLabel}`}
                </span>
                <span>{Math.floor(progress)}%</span>
              </div>
            </div>
            {progress < 100 && (
              <button className="ob-btn-ghost" onClick={handleCancel}>
                Cancel
              </button>
            )}
          </>
        )}
      </div>

      <div className="ob-step-dots">
        <span className="ob-dot" />
        <span className="ob-dot active" />
        <span className="ob-dot" />
        <span className="ob-dot" />
      </div>
    </div>
  );
}
