import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Droid from "../components/Droid/Droid";

interface AccessibilityPermissionProps {
  onNext: () => void;
}

export default function AccessibilityPermission({ onNext }: AccessibilityPermissionProps) {
  // "checking" until the initial trust check resolves, so we never flash the
  // permission screen when it's already granted.
  const [checking, setChecking] = useState(true);
  const [waiting, setWaiting] = useState(false);
  const [needsRestart, setNeedsRestart] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // On mount: if already granted, skip straight to the next step.
  useEffect(() => {
    invoke<boolean>("check_accessibility_permission")
      .then((trusted) => {
        if (trusted) onNext();
        else setChecking(false);
      })
      .catch(() => setChecking(false));
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, []);

  async function handleContinue() {
    // Show macOS's own Accessibility prompt (deep-links to Settings).
    await invoke("prompt_accessibility_permission").catch(() => null);
    setWaiting(true);

    let ticks = 0;
    pollRef.current = setInterval(async () => {
      ticks++;
      const trusted = await invoke<boolean>("check_accessibility_permission").catch(() => false);
      if (trusted) {
        if (pollRef.current) clearInterval(pollRef.current);
        onNext();
      } else if (ticks >= 5) {
        // The grant often only takes effect on a fresh launch — offer a restart.
        setNeedsRestart(true);
      }
    }, 1000);
  }

  function handleRestart() {
    invoke("restart_app").catch(() => null);
  }

  if (checking) {
    return (
      <div className="onboarding-root ob-screen-enter">
        <div className="ob-droid-stage">
          <Droid state="thinking" size={100} />
        </div>
      </div>
    );
  }

  return (
    <div className="onboarding-root ob-screen-enter">
      <div className="ob-droid-stage">
        <Droid state="listening" size={100} />
        <div className="ob-bubble">
          I need permission to{" "}
          <strong>read and type in other apps</strong> —
          that's how I edit your emails and fill forms.
        </div>
      </div>

      <div className="ob-content">
        <div className="ob-permission-box">
          <strong>What this lets me do</strong>
          When you ask me to edit an email, I read the text
          from whichever app you're typing in and write your
          changes back — instantly, only when you ask.
        </div>

        <div className="ob-permission-box" style={{ borderColor: "rgba(0,180,255,0.25)", background: "rgba(0,180,255,0.06)" }}>
          <strong style={{ color: "var(--accent)" }}>What happens next</strong>
          Your Mac will ask you to confirm — click{" "}
          <strong>Open System Settings</strong>, then switch{" "}
          <strong>AI Buddy</strong> on in the Accessibility list.
        </div>

        {!waiting && (
          <button className="ob-btn-primary" onClick={handleContinue}>
            Continue →
          </button>
        )}

        {waiting && !needsRestart && (
          <>
            <button className="ob-btn-primary" disabled>
              Waiting for permission…
            </button>
            <p style={{ textAlign: "center", fontSize: "0.8rem", opacity: 0.5, marginTop: "0.5rem" }}>
              Switch AI Buddy on in System Settings → Privacy & Security → Accessibility.
            </p>
          </>
        )}

        {needsRestart && (
          <>
            <button className="ob-btn-primary" onClick={handleRestart}>
              Restart AI Buddy to finish
            </button>
            <p style={{ textAlign: "center", fontSize: "0.8rem", opacity: 0.5, marginTop: "0.5rem" }}>
              Already enabled it? A quick restart lets the new permission take effect.
            </p>
          </>
        )}

        {!waiting && (
          <button
            className="ob-btn-ghost"
            style={{ marginTop: "0.5rem", fontSize: "0.8rem", opacity: 0.5 }}
            onClick={onNext}
          >
            Skip for now →
          </button>
        )}
      </div>

      <div className="ob-step-dots">
        <span className="ob-dot" />
        <span className="ob-dot" />
        <span className="ob-dot active" />
        <span className="ob-dot" />
      </div>
    </div>
  );
}
