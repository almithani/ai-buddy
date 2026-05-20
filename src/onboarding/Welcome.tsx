import Droid from "../components/Droid/Droid";
import "../onboarding/onboarding.css";

interface WelcomeProps {
  onNext: () => void;
}

export default function Welcome({ onNext }: WelcomeProps) {
  return (
    <div className="onboarding-root ob-screen-enter">
      <div className="ob-droid-stage">
        <Droid state="idle" size={130} />
        <div className="ob-bubble">
          Hi! I'm <strong>AI Buddy</strong>.<br />
          I live on your screen and help you<br />
          get everyday things done — faster.
        </div>
      </div>

      <div className="ob-content">
        <p style={{ color: "var(--text-muted)", fontSize: 13, textAlign: "center", maxWidth: 280 }}>
          I work entirely on your device.<br />
          No accounts. No internet required.<br />
          Your data stays with you.
        </p>

        <button className="ob-btn-primary" onClick={onNext}>
          Let's get started →
        </button>
      </div>

      <div className="ob-step-dots">
        <span className="ob-dot active" />
        <span className="ob-dot" />
        <span className="ob-dot" />
        <span className="ob-dot" />
      </div>
    </div>
  );
}
