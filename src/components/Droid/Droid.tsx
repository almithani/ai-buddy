import "./Droid.css";

export type DroidState =
  | "idle"
  | "listening"
  | "thinking"
  | "working"
  | "done"
  | "error";

export type TaskIcon =
  | "file"
  | "calendar"
  | "email"
  | "spreadsheet"
  | "transcription"
  | "web"
  | "thinking"
  | null;

interface DroidProps {
  state?: DroidState;
  taskIcon?: TaskIcon;
  size?: number;
}

const TASK_ICON_GLYPHS: Record<NonNullable<TaskIcon>, string> = {
  file:          "📄",
  calendar:      "📅",
  email:         "✉️",
  spreadsheet:   "📊",
  transcription: "🎙️",
  web:           "🌐",
  thinking:      "💭",
};

export default function Droid({ state = "idle", taskIcon = null, size = 100 }: DroidProps) {
  return (
    <div className={`droid-wrapper droid-${state}`} style={{ width: size, height: size }}>
      <svg
        viewBox="0 0 100 100"
        width={size}
        height={size}
        className="droid-svg"
        aria-label={`AI Buddy — ${state}`}
      >
        <defs>
          <radialGradient id="bodyGrad" cx="35%" cy="28%" r="72%">
            <stop offset="0%"   stopColor="#FFFFFF" />
            <stop offset="55%"  stopColor="#EAF0F8" />
            <stop offset="100%" stopColor="#C8D4E4" />
          </radialGradient>

          <radialGradient id="eyeGrad" cx="30%" cy="30%" r="70%">
            <stop offset="0%"   stopColor="#60D4FF" />
            <stop offset="100%" stopColor="#0077CC" />
          </radialGradient>

          <filter id="bodyShadow" x="-25%" y="-20%" width="150%" height="160%">
            <feDropShadow dx="0" dy="5" stdDeviation="7"
              floodColor="rgba(0, 80, 180, 0.22)" />
          </filter>

          <filter id="eyeGlow" x="-60%" y="-60%" width="220%" height="220%">
            <feGaussianBlur stdDeviation="2.5" result="blur" />
            <feMerge>
              <feMergeNode in="blur" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
        </defs>

        {/* Body */}
        <ellipse
          className="droid-body"
          cx="50" cy="56"
          rx="30" ry="34"
          fill="url(#bodyGrad)"
          filter="url(#bodyShadow)"
        />

        {/* Head */}
        <ellipse
          className="droid-body"
          cx="50" cy="36"
          rx="38" ry="30"
          fill="url(#bodyGrad)"
          filter="url(#bodyShadow)"
        />

        {/* Left eye */}
        <g className="droid-eye droid-eye-left" filter="url(#eyeGlow)">
          <ellipse cx="33" cy="37" rx="12" ry="11" fill="url(#eyeGrad)" className="eye-fill" />
          {/* Pupil / inner glow */}
          <ellipse cx="33" cy="37" rx="6" ry="6" fill="white" opacity="0.18" />
          {/* Highlight */}
          <ellipse cx="29" cy="33" rx="3" ry="2.2" fill="white" opacity="0.65" />
        </g>

        {/* Right eye */}
        <g className="droid-eye droid-eye-right" filter="url(#eyeGlow)">
          <ellipse cx="67" cy="37" rx="12" ry="11" fill="url(#eyeGrad)" className="eye-fill" />
          <ellipse cx="67" cy="37" rx="6" ry="6" fill="white" opacity="0.18" />
          <ellipse cx="63" cy="33" rx="3" ry="2.2" fill="white" opacity="0.65" />
        </g>

        {/* Thinking ring (visible only in thinking state) */}
        <circle
          className="droid-think-ring"
          cx="50" cy="36"
          r="43"
          fill="none"
          stroke="rgba(0,180,255,0.4)"
          strokeWidth="2"
          strokeDasharray="15 10"
        />
      </svg>

      {/* Task icon badge */}
      {taskIcon && (
        <div className="droid-task-icon" aria-hidden="true">
          {TASK_ICON_GLYPHS[taskIcon]}
        </div>
      )}
    </div>
  );
}
