import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ShieldCheck, TriangleAlert } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import OnboardingShell from "./OnboardingShell";

interface RecordingConsentSetupProps {
  onComplete: () => void;
}

function RecordingConsentSetup({ onComplete }: RecordingConsentSetupProps) {
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleAcknowledge() {
    setSubmitting(true);
    setError(null);
    try {
      await invoke("set_recording_consent");
      onComplete();
    } catch (err) {
      const e = err as { message?: string };
      setError(
        e.message ?? "Could not save your acknowledgement. Please try again.",
      );
      setSubmitting(false);
    }
  }

  return (
    <OnboardingShell
      icon={<ShieldCheck strokeWidth={1.75} />}
      title="Record responsibly"
      description="Recording laws vary by state and country. In some places you must have the consent of everyone being recorded."
    >
      <Alert className="text-left p-4 bg-muted/50">
        <TriangleAlert />
        {/* <AlertTitle>You are responsible for recording legally</AlertTitle> */}
        <AlertDescription>
          You are responsible for complying with the recording laws that apply
          to you, including obtaining any required consent from participants.
          TuckNotes does not provide legal advice.
        </AlertDescription>
      </Alert>

      {error && <p className="text-sm text-destructive">{error}</p>}

      <Button
        className="w-full h-11 rounded-xl text-[0.95rem] font-semibold"
        onClick={handleAcknowledge}
        disabled={submitting}
      >
        I Understand
      </Button>
    </OnboardingShell>
  );
}

export default RecordingConsentSetup;
