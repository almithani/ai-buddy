import { invoke } from "@tauri-apps/api/core";
import Droid from "../components/Droid/Droid";

interface AccessibilityPermissionProps {
  onNext: () => void;
}

export default function AccessibilityPermission({ onNext }: AccessibilityPermissionProps) {
  async function handleContinue() {
    await invoke("request_accessibility_permission");
    // Give the user a moment to interact with the OS dialog, then proceed.
    // In milestone 2 we'll poll for the actual permission grant.
    setTimeout(onNext, 800);
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

        <button className="ob-btn-primary" onClick={handleContinue}>
          Continue →
        </button>
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
