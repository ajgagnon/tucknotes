import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import PermissionSetup from "@/features/onboarding/PermissionSetup";
import RecordingConsentSetup from "@/features/onboarding/RecordingConsentSetup";
import ModelSetup from "@/features/onboarding/ModelSetup";
import SummarizationSetup from "@/features/onboarding/SummarizationSetup";
import AppLayout from "@/layout/AppLayout";
import { useAutoUpdateCheck } from "@/hooks/use-auto-update-check";

type OnboardingStep =
  | "loading"
  | "permissions"
  | "recording-consent"
  | "model-setup"
  | "summarization-setup"
  | "ready";

function App() {
  const [step, setStep] = useState<OnboardingStep>("loading");
  useAutoUpdateCheck({ enabled: step === "ready" });

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

      setStep(await nextStepAfterPermissions());
    }

    checkOnboarding().catch(() => setStep("permissions"));
  }, []);

  async function checkModelReady(): Promise<boolean> {
    const selected = await invoke<string | null>("get_selected_model");
    if (!selected) return false;
    return invoke<boolean>("get_model_status", { modelId: selected });
  }

  async function checkSummarizationReady(): Promise<boolean> {
    const selected = await invoke<string | null>("get_selected_llm_model");
    return selected !== null;
  }

  async function checkConsentReady(): Promise<boolean> {
    return invoke<boolean>("get_recording_consent");
  }

  async function nextStepAfterPermissions(): Promise<OnboardingStep> {
    if (!(await checkConsentReady())) return "recording-consent";
    if (!(await checkModelReady())) return "model-setup";
    if (!(await checkSummarizationReady())) return "summarization-setup";
    return "ready";
  }

  async function handlePermissionsComplete() {
    setStep(await nextStepAfterPermissions());
  }

  async function handleModelSetupComplete() {
    const summReady = await checkSummarizationReady();
    setStep(summReady ? "ready" : "summarization-setup");
  }

  if (step === "loading") return null;
  if (step === "permissions") {
    return <PermissionSetup onComplete={handlePermissionsComplete} />;
  }
  if (step === "recording-consent") {
    return (
      <RecordingConsentSetup
        onComplete={async () => setStep(await nextStepAfterPermissions())}
      />
    );
  }
  if (step === "model-setup") {
    return <ModelSetup onComplete={handleModelSetupComplete} />;
  }
  if (step === "summarization-setup") {
    return <SummarizationSetup onComplete={() => setStep("ready")} />;
  }
  return <AppLayout />;
}

export default App;
