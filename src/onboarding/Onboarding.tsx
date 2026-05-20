import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Welcome from "./Welcome";
import ModelDownload from "./ModelDownload";
import AccessibilityPermission from "./AccessibilityPermission";
import Ready from "./Ready";

type Step = "welcome" | "download" | "permission" | "ready";

export default function OnboardingFlow() {
  const [step, setStep] = useState<Step>("welcome");
  const [platform, setPlatform] = useState<string>("unknown");

  useEffect(() => {
    invoke<string>("get_platform").then(setPlatform).catch(() => null);
  }, []);

  function afterDownload() {
    // Linux doesn't need an accessibility permission prompt (AT-SPI is always on)
    if (platform === "linux") {
      setStep("ready");
    } else {
      setStep("permission");
    }
  }

  if (step === "welcome")    return <Welcome onNext={() => setStep("download")} />;
  if (step === "download")   return <ModelDownload onNext={afterDownload} />;
  if (step === "permission") return <AccessibilityPermission onNext={() => setStep("ready")} />;
  return <Ready />;
}
