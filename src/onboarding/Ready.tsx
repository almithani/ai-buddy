import { invoke } from "@tauri-apps/api/core";
import Droid from "../components/Droid/Droid";

export default function Ready() {
  async function handleStart() {
    await invoke("complete_onboarding");
  }

  return (
    <div className="onboarding-root ob-screen-enter">
      <div className="ob-droid-stage">
        <Droid state="done" size={130} />
        <div className="ob-bubble">
          All set! I'm ready whenever you are.
        </div>
      </div>

      <div className="ob-content">
        <ul className="ob-ready-list">
          <li>
            <span className="ob-check">✓</span>
            AI model downloaded and ready
          </li>
          <li>
            <span className="ob-check">✓</span>
            Can read and type in any app
          </li>
          <li>
            <span className="ob-check">✓</span>
            Everything runs on your device
          </li>
        </ul>

        <p style={{ color: "var(--text-muted)", fontSize: 12.5, textAlign: "center", maxWidth: 270, marginTop: 4 }}>
          I'll appear as a small icon in the corner of your screen.
          Click me anytime to chat, or drag and drop files onto me.
        </p>

        <button className="ob-btn-primary" onClick={handleStart}>
          Start using AI Buddy
        </button>
      </div>

      <div className="ob-step-dots">
        <span className="ob-dot" />
        <span className="ob-dot" />
        <span className="ob-dot" />
        <span className="ob-dot active" />
      </div>
    </div>
  );
}
