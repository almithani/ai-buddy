import { getCurrentWindow } from "@tauri-apps/api/window";
import OnboardingFlow from "./onboarding/Onboarding";
import DroidOverlay from "./components/Droid/DroidOverlay";
import ChatPanel from "./components/ChatPanel/ChatPanel";
import Highlight from "./components/Highlight/Highlight";
import { followDisplayPrefs } from "./lib/displayPrefs";

const label = getCurrentWindow().label;
followDisplayPrefs(label === "chat");

export default function App() {
  if (label === "droid") return <DroidOverlay />;
  if (label === "chat") return <ChatPanel />;
  if (label === "highlight") return <Highlight />;
  return <OnboardingFlow />;
}
