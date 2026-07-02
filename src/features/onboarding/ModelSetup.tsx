import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "@/hooks/use-tauri-event";
import { Mic } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { ModelInfo, DownloadProgress } from "@/features/models";
import { formatSize } from "@/features/models";
import OnboardingShell from "./OnboardingShell";

interface ModelSetupProps {
  onComplete: () => void;
}

function ModelSetup({ onComplete }: ModelSetupProps) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<ModelInfo[]>("list_available_models").then((list) => {
      setModels(list);
      const initial = list.find((m) => m.recommended) ?? list[0];
      if (initial) setSelectedId(initial.id);
    });
  }, []);

  useTauriEvent<DownloadProgress>(
    "model:download-progress",
    (progress) => setProgress(progress),
    { enabled: downloading },
  );

  async function handleDownload() {
    if (!selectedId) return;
    setDownloading(true);
    setError(null);
    setProgress(null);
    try {
      await invoke("download_model", { modelId: selectedId });
      await invoke("set_selected_model", { modelId: selectedId });
      onComplete();
    } catch (err) {
      const e = err as { kind: string; message?: string };
      setError(e.message ?? "Download failed. Please try again.");
      setDownloading(false);
    }
  }

  const progressPercent =
    progress && progress.total_bytes > 0
      ? Math.min((progress.downloaded_bytes / progress.total_bytes) * 100, 100)
      : 0;

  if (downloading) {
    const selectedModel = models.find((m) => m.id === selectedId);
    return (
      <OnboardingShell
        icon={<Mic strokeWidth={1.75} />}
        title="Downloading model"
        description={
          <>
            {selectedModel?.name ?? "Model"} —{" "}
            {formatSize(selectedModel?.size_bytes ?? 0)}
          </>
        }
      >
        <div className="w-full flex flex-col gap-3">
          <div className="w-full bg-muted rounded-full h-2 overflow-hidden">
            <div
              className="bg-primary h-full rounded-full transition-all duration-300 ease-out"
              style={{ width: `${progressPercent}%` }}
            />
          </div>

          <p className="text-sm text-muted-foreground tabular-nums">
            {progress
              ? `${formatSize(progress.downloaded_bytes)} / ${formatSize(progress.total_bytes)}`
              : "Starting download…"}
          </p>

          {error && (
            <div className="flex flex-col gap-3 items-center mt-2">
              <p className="text-sm text-destructive">{error}</p>
              <Button variant="outline" onClick={handleDownload}>
                Retry
              </Button>
            </div>
          )}
        </div>
      </OnboardingShell>
    );
  }

  return (
    <OnboardingShell
      icon={<Mic strokeWidth={1.75} />}
      title="Choose a transcription model"
      description="Select a Whisper model to power your meeting notes. You can change this later in settings."
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
                  <h3 className="text-[0.95rem] font-semibold">
                    {model.name}
                  </h3>
                  {model.recommended && (
                    <Badge variant="outline">Recommended</Badge>
                  )}
                </div>
                <div
                  className={`w-5 h-5 rounded-full border-2 flex items-center justify-center shrink-0 transition-colors ${
                    isSelected
                      ? "border-primary bg-primary"
                      : "border-border"
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

      <Button
        className="w-full h-11 rounded-xl text-[0.95rem] font-semibold"
        onClick={handleDownload}
        disabled={!selectedId}
      >
        Download & Continue
      </Button>
    </OnboardingShell>
  );
}

export default ModelSetup;
