import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Sun, Moon, Monitor } from "lucide-react";
import { Skeleton } from "@/components/ui/skeleton";
import { Badge } from "@/components/ui/badge";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
  FieldTitle,
} from "@/components/ui/field";
import type { ModelInfo, LlmModelInfo, DownloadProgress } from "@/lib/models";
import { formatSize } from "@/lib/models";
import {
  type Theme,
  getStoredTheme,
  setStoredTheme,
  applyTheme,
} from "@/lib/theme";

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
  const [theme, setTheme] = useState<Theme>(getStoredTheme);

  // LLM model state
  const [llmModels, setLlmModels] = useState<LlmModelInfo[]>([]);
  const [llmDownloadStatus, setLlmDownloadStatus] = useState<
    Record<string, boolean>
  >({});
  const [llmDownloading, setLlmDownloading] = useState<string | null>(null);
  const [llmProgress, setLlmProgress] = useState<DownloadProgress | null>(
    null,
  );
  const [llmError, setLlmError] = useState<string | null>(null);
  const [selectedLlmModelId, setSelectedLlmModelId] = useState<string | null>(
    null,
  );

  useEffect(() => {
    async function load() {
      try {
        const [list, selected, llmList, selectedLlm] = await Promise.all([
          invoke<ModelInfo[]>("list_available_models"),
          invoke<string | null>("get_selected_model"),
          invoke<LlmModelInfo[]>("list_available_llm_models"),
          invoke<string | null>("get_selected_llm_model"),
        ]);
        setModels(list);
        setCurrentModelId(selected);
        setLlmModels(llmList);
        setSelectedLlmModelId(selectedLlm);

        const statuses: Record<string, boolean> = {};
        await Promise.all(
          list.map(async (m) => {
            statuses[m.id] = await invoke<boolean>("get_model_status", {
              modelId: m.id,
            });
          }),
        );
        setDownloadStatus(statuses);

        const llmStatuses: Record<string, boolean> = {};
        await Promise.all(
          llmList.map(async (m) => {
            llmStatuses[m.id] = await invoke<boolean>("get_llm_model_status", {
              modelId: m.id,
            });
          }),
        );
        setLlmDownloadStatus(llmStatuses);
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

  useEffect(() => {
    if (!llmDownloading) return;
    const unlisten = listen<DownloadProgress>(
      "llm-model:download-progress",
      (event) => setLlmProgress(event.payload),
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [llmDownloading]);

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

  /** Same interaction pattern as `handleSelectModel`: pick downloaded model, or download then select. */
  async function handleSelectLlmModel(modelId: string) {
    if (modelId === selectedLlmModelId || llmDownloading) return;

    const isDownloaded = llmDownloadStatus[modelId] ?? false;

    if (isDownloaded) {
      try {
        await invoke("set_selected_llm_model", { modelId });
        setSelectedLlmModelId(modelId);
        setLlmError(null);
      } catch (err) {
        const e = err as { message?: string };
        setLlmError(e.message ?? "Failed to switch model.");
      }
    } else {
      setLlmDownloading(modelId);
      setLlmError(null);
      setLlmProgress(null);
      try {
        await invoke("download_llm_model", { modelId });
        await invoke("set_selected_llm_model", { modelId });
        setSelectedLlmModelId(modelId);
        setLlmDownloadStatus((prev) => ({ ...prev, [modelId]: true }));
      } catch (err) {
        const e = err as { message?: string };
        setLlmError(e.message ?? "Download failed. Please try again.");
      } finally {
        setLlmDownloading(null);
        setLlmProgress(null);
      }
    }
  }

  const llmProgressPercent =
    llmProgress && llmProgress.total_bytes > 0
      ? Math.min(
          (llmProgress.downloaded_bytes / llmProgress.total_bytes) * 100,
          100,
        )
      : 0;

  const progressPercent =
    progress && progress.total_bytes > 0
      ? Math.min((progress.downloaded_bytes / progress.total_bytes) * 100, 100)
      : 0;

  return (
    <div className="h-full overflow-auto">
      <div className="max-w-2xl mx-auto p-8">
        {/* Appearance */}
        <section className="mb-8">
          <h2 className="text-sm font-medium text-muted-foreground mb-4">
            Appearance
          </h2>
          <ToggleGroup
            variant="outline"
            value={[theme]}
            onValueChange={(newValue) => {
              const next = newValue.find((v) => v !== theme) as
                | Theme
                | undefined;
              if (!next) return; // ignore deselect
              setTheme(next);
              setStoredTheme(next);
              applyTheme(next);
            }}
          >
            <ToggleGroupItem value="light">
              <Sun className="size-4 mr-1" />
              Light
            </ToggleGroupItem>
            <ToggleGroupItem value="dark">
              <Moon className="size-4 mr-1" />
              Dark
            </ToggleGroupItem>
            <ToggleGroupItem value="system">
              <Monitor className="size-4 mr-1" />
              System
            </ToggleGroupItem>
          </ToggleGroup>
        </section>

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

        {/* Summarization Model — same layout as Transcription Model */}
        <section className="mt-8">
          <h2 className="text-sm font-medium text-muted-foreground mb-4">
            Summarization Model
          </h2>

          {loading ? (
            <div className="flex flex-col gap-3">
              <Skeleton className="h-20 rounded-lg" />
              <Skeleton className="h-20 rounded-lg" />
            </div>
          ) : (
            <RadioGroup
              value={selectedLlmModelId ?? undefined}
              onValueChange={handleSelectLlmModel}
              disabled={!!llmDownloading}
            >
              {llmModels.map((model) => {
                const isDownloaded = llmDownloadStatus[model.id] ?? false;
                const isDownloading = llmDownloading === model.id;

                return (
                  <FieldLabel key={model.id} htmlFor={`llm-model-${model.id}`}>
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

                        {isDownloading && (
                          <div className="mt-1.5">
                            <div className="w-full bg-neutral-200 dark:bg-neutral-700 rounded-full h-1.5 mb-1 overflow-hidden">
                              <div
                                className="bg-primary h-full rounded-full transition-all duration-300 ease-out"
                                style={{ width: `${llmProgressPercent}%` }}
                              />
                            </div>
                            <p className="text-xs text-muted-foreground tabular-nums">
                              {llmProgress
                                ? `${formatSize(llmProgress.downloaded_bytes)} / ${formatSize(llmProgress.total_bytes)}`
                                : "Starting download\u2026"}
                            </p>
                          </div>
                        )}

                        {llmError &&
                          !llmDownloading &&
                          selectedLlmModelId !== model.id && (
                            <p className="text-xs text-destructive mt-1">
                              {llmError}
                            </p>
                          )}
                      </FieldContent>
                      <RadioGroupItem
                        value={model.id}
                        id={`llm-model-${model.id}`}
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
