import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DownloadCloud, Sparkles } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { ModelInfo } from "@/features/models";
import { formatSize } from "@/features/models";
import OnboardingShell from "./OnboardingShell";

interface SummarizationSetupProps {
  onComplete: () => void;
}

function SummarizationSetup({ onComplete }: SummarizationSetupProps) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<ModelInfo[]>("list_available_llm_models").then((list) => {
      setModels(list);
      const initial = list.find((m) => m.recommended) ?? list[0];
      if (initial) setSelectedId(initial.id);
    });
  }, []);

  async function handleContinue() {
    if (!selectedId) return;
    setSubmitting(true);
    setError(null);
    try {
      await invoke("set_selected_llm_model", { modelId: selectedId });
      // Fire-and-forget: the Tauri command keeps running on the tokio runtime
      // after we abandon the Promise, so the download persists into the app.
      void invoke("download_llm_model", { modelId: selectedId }).catch(
        () => {},
      );
      onComplete();
    } catch (err) {
      const e = err as { message?: string };
      setError(e.message ?? "Could not save your selection. Please try again.");
      setSubmitting(false);
    }
  }

  return (
    <OnboardingShell
      icon={<Sparkles strokeWidth={1.75} />}
      title="Choose a summarization model"
      description="Used to summarize and title your meeting transcripts."
    >
      <div className="flex flex-col gap-3 w-full text-left">
        {models.map((model) => {
          const isSelected = selectedId === model.id;
          return (
            <button
              key={model.id}
              onClick={() => setSelectedId(model.id)}
              className={`rounded-xl p-5 px-6 text-left transition-all duration-200 cursor-pointer ${
                isSelected
                  ? "border-2 border-primary bg-primary/5 dark:bg-primary/10"
                  : "border border-border bg-muted/40 hover:border-foreground/20"
              }`}
            >
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-2">
                  <h3 className="text-[0.95rem] font-semibold">{model.name}</h3>
                  {model.recommended && (
                    <Badge variant="outline">Recommended</Badge>
                  )}
                </div>
                <div
                  className={`w-5 h-5 rounded-full border-2 flex items-center justify-center shrink-0 transition-colors ${
                    isSelected ? "border-primary bg-primary" : "border-border"
                  }`}
                >
                  {isSelected && (
                    <div className="w-2 h-2 rounded-full bg-primary-foreground" />
                  )}
                </div>
              </div>
              <p className="text-sm text-muted-foreground leading-relaxed mb-2">
                {model.description}
              </p>
              <span className="text-xs font-medium text-muted-foreground">
                {formatSize(model.size_bytes)}
              </span>
            </button>
          );
        })}
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}

      <Button
        className="w-full h-11 rounded-xl text-[0.95rem] font-semibold"
        onClick={handleContinue}
        disabled={!selectedId || submitting}
      >
        Choose & Continue
      </Button>

      <Alert className="text-left">
        <DownloadCloud />
        <AlertTitle>Downloads in the background</AlertTitle>
        <AlertDescription>
          You can start using the app right away — summarization will be
          available once the download finishes.
        </AlertDescription>
      </Alert>
    </OnboardingShell>
  );
}

export default SummarizationSetup;
