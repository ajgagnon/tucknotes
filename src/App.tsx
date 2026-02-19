import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import PermissionSetup from "./components/PermissionSetup";
import ModelSetup from "./components/ModelSetup";
import RecordingView from "./components/RecordingView";

type OnboardingStep = "loading" | "permissions" | "model-setup" | "ready";

function App() {
  const [step, setStep] = useState<OnboardingStep>("loading");

  useEffect(() => {
    async function checkOnboarding() {
      const [screen, mic] = await Promise.all([
        invoke<boolean>("check_screen_recording_permission"),
        invoke<string>("check_microphone_permission"),
      ]);
      const permissionsGranted = screen && mic === "authorized";
      if (!permissionsGranted) {
        setStep("permissions");
        return;
      }

      const modelReady = await checkModelReady();
      setStep(modelReady ? "ready" : "model-setup");
    }

    checkOnboarding().catch(() => setStep("permissions"));
  }, []);

  async function checkModelReady(): Promise<boolean> {
    const selected = await invoke<string | null>("get_selected_model");
    if (!selected) return false;
    return invoke<boolean>("get_model_status", { modelId: selected });
  }

  async function handlePermissionsComplete() {
    const modelReady = await checkModelReady();
    setStep(modelReady ? "ready" : "model-setup");
  }

  if (step === "loading") return null;
  if (step === "permissions") {
    return <PermissionSetup onComplete={handlePermissionsComplete} />;
  }
  if (step === "model-setup") {
    return <ModelSetup onComplete={() => setStep("ready")} />;
  }
  return <RecordingView />;
}

export default App;
