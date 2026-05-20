import { getCurrentWindow } from "@tauri-apps/api/window";
import OnboardingFlow from "./onboarding/Onboarding";
import DroidOverlay from "./components/Droid/DroidOverlay";
import ChatPanel from "./components/ChatPanel/ChatPanel";

const label = getCurrentWindow().label;

export default function App() {
  if (label === "droid") return <DroidOverlay />;
  if (label === "chat") return <ChatPanel />;
  return <OnboardingFlow />;
}
