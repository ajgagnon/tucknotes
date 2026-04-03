import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import PermissionSetup from "@/features/onboarding/PermissionSetup";
import ModelSetup from "@/features/onboarding/ModelSetup";
import AppLayout from "@/layout/AppLayout";

type OnboardingStep = "loading" | "permissions" | "model-setup" | "ready";

function App() {
  const [step, setStep] = useState<OnboardingStep>("loading");

  useEffect(() => {
    async function checkOnboarding() {
      const [screen, mic, accessibility] = await Promise.all([
        invoke<boolean>("check_screen_recording_permission"),
        invoke<string>("check_microphone_permission"),
        invoke<boolean>("check_accessibility_permission"),
      ]);
      const permissionsGranted =
        screen && mic === "authorized" && accessibility;
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
  return <AppLayout />;
}

export default App;
