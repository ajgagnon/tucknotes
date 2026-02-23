import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Skeleton } from "@/components/ui/skeleton";
import { Badge } from "@/components/ui/badge";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
  FieldTitle,
} from "@/components/ui/field";
import type { ModelInfo, DownloadProgress } from "@/lib/models";
import { formatSize } from "@/lib/models";

function SettingsView() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [currentModelId, setCurrentModelId] = useState<string | null>(null);
  const [downloadStatus, setDownloadStatus] = useState<Record<string, boolean>>(
    {},
  );
  const [loading, setLoading] = useState(true);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    async function load() {
      try {
        const [list, selected] = await Promise.all([
          invoke<ModelInfo[]>("list_available_models"),
          invoke<string | null>("get_selected_model"),
        ]);
        setModels(list);
        setCurrentModelId(selected);

        const statuses: Record<string, boolean> = {};
        await Promise.all(
          list.map(async (m) => {
            statuses[m.id] = await invoke<boolean>("get_model_status", {
              modelId: m.id,
            });
          }),
        );
        setDownloadStatus(statuses);
      } finally {
        setLoading(false);
      }
    }
    load();
  }, []);

  useEffect(() => {
    if (!downloading) return;
    const unlisten = listen<DownloadProgress>(
      "model:download-progress",
      (event) => setProgress(event.payload),
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [downloading]);

  async function handleSelectModel(modelId: string) {
    if (modelId === currentModelId || downloading) return;

    const isDownloaded = downloadStatus[modelId] ?? false;

    if (isDownloaded) {
      try {
        await invoke("set_selected_model", { modelId });
        setCurrentModelId(modelId);
      } catch (err) {
        const e = err as { message?: string };
        setError(e.message ?? "Failed to switch model.");
      }
    } else {
      setDownloading(modelId);
      setError(null);
      setProgress(null);
      try {
        await invoke("download_model", { modelId });
        await invoke("set_selected_model", { modelId });
        setCurrentModelId(modelId);
        setDownloadStatus((prev) => ({ ...prev, [modelId]: true }));
      } catch (err) {
        const e = err as { message?: string };
        setError(e.message ?? "Download failed. Please try again.");
      } finally {
        setDownloading(null);
        setProgress(null);
      }
    }
  }

  const progressPercent =
    progress && progress.total_bytes > 0
      ? Math.min((progress.downloaded_bytes / progress.total_bytes) * 100, 100)
      : 0;

  return (
    <div className="h-full overflow-auto">
      <div className="max-w-2xl mx-auto p-8">
        {/* Transcription Model */}
        <section>
          <h2 className="text-sm font-medium text-muted-foreground mb-4">
            Transcription Model
          </h2>

          {loading ? (
            <div className="flex flex-col gap-3">
              <Skeleton className="h-20 rounded-lg" />
              <Skeleton className="h-20 rounded-lg" />
            </div>
          ) : (
            <RadioGroup
              value={currentModelId ?? undefined}
              onValueChange={handleSelectModel}
              disabled={!!downloading}
            >
              {models.map((model) => {
                const isDownloaded = downloadStatus[model.id] ?? false;
                const isDownloading = downloading === model.id;

                return (
                  <FieldLabel key={model.id} htmlFor={`model-${model.id}`}>
                    <Field orientation="horizontal">
                      <FieldContent>
                        <FieldTitle>
                          {model.name}
                          {model.recommended && (
                            <Badge variant="outline">Recommended</Badge>
                          )}
                        </FieldTitle>
                        <FieldDescription>
                          {model.description}
                          <span className="text-muted-foreground/60">
                            {" "}
                            &middot; {formatSize(model.size_bytes)}
                          </span>
                          {!isDownloaded && !isDownloading && (
                            <span className="text-muted-foreground/60">
                              {" "}
                              &middot; Download required
                            </span>
                          )}
                        </FieldDescription>

                        {/* Download progress */}
                        {isDownloading && (
                          <div className="mt-1.5">
                            <div className="w-full bg-neutral-200 dark:bg-neutral-700 rounded-full h-1.5 mb-1 overflow-hidden">
                              <div
                                className="bg-primary h-full rounded-full transition-all duration-300 ease-out"
                                style={{ width: `${progressPercent}%` }}
                              />
                            </div>
                            <p className="text-xs text-muted-foreground tabular-nums">
                              {progress
                                ? `${formatSize(progress.downloaded_bytes)} / ${formatSize(progress.total_bytes)}`
                                : "Starting download\u2026"}
                            </p>
                          </div>
                        )}

                        {/* Error */}
                        {error &&
                          !downloading &&
                          currentModelId !== model.id && (
                            <p className="text-xs text-destructive mt-1">
                              {error}
                            </p>
                          )}
                      </FieldContent>
                      <RadioGroupItem
                        value={model.id}
                        id={`model-${model.id}`}
                      />
                    </Field>
                  </FieldLabel>
                );
              })}
            </RadioGroup>
          )}
        </section>
      </div>
    </div>
  );
}

export default SettingsView;
