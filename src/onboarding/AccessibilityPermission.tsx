import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Droid from "../components/Droid/Droid";

interface AccessibilityPermissionProps {
  onNext: () => void;
}

export default function AccessibilityPermission({ onNext }: AccessibilityPermissionProps) {
  const [waiting, setWaiting] = useState(false);

  async function handleContinue() {
    await invoke("request_accessibility_permission");
    setWaiting(true);
    const poll = setInterval(async () => {
      const trusted = await invoke<boolean>("check_accessibility_permission");
      if (trusted) {
        clearInterval(poll);
        onNext();
      }
    }, 1000);
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
          Your computer will ask you to confirm.
          Just click <strong>Allow</strong>.
        </div>

        <button
          className="ob-btn-primary"
          onClick={handleContinue}
          disabled={waiting}
        >
          {waiting ? "Waiting for permission…" : "Continue →"}
        </button>

        {waiting && (
          <p style={{ textAlign: "center", fontSize: "0.8rem", opacity: 0.5, marginTop: "0.5rem" }}>
            Waiting for you to grant access in System Settings…
          </p>
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
