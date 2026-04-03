import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  Sun,
  Moon,
  Monitor,
  MoreVertical,
  Trash2,
  FolderOpen,
  Download,
} from "lucide-react";
import { Button } from "@/components/ui/button";
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

/** Menu label for revealing the model file in the system file manager. */
function revealInFolderMenuLabel(): string {
  if (typeof navigator === "undefined") return "Show in folder";
  const p = navigator.platform;
  if (p.startsWith("Mac") || p === "iPhone") return "Show in Finder";
  if (p.startsWith("Win")) return "Show in File Explorer";
  return "Show in folder";
}

/** Stops events from reaching the parent FieldLabel (label) so the radio does not toggle. */
function stopLabelBubbling(e: React.SyntheticEvent) {
  e.stopPropagation();
}

function ModelActionsMenu({
  onShowInFolder,
  onRemove,
  canRemove,
}: {
  onShowInFolder: () => void;
  onRemove: () => void;
  /** When false, Remove is hidden (e.g. active model). */
  canRemove: boolean;
}) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  return (
    <div
      className="relative shrink-0"
      ref={menuRef}
      data-slot="model-actions-menu"
      onPointerDown={stopLabelBubbling}
      onClick={stopLabelBubbling}
    >
      <button
        type="button"
        onClick={(e) => {
          e.preventDefault();
          stopLabelBubbling(e);
          setOpen((prev) => !prev);
        }}
        className="p-1.5 rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-ring"
        aria-label="Model options"
        aria-expanded={open}
      >
        <MoreVertical className="size-4" />
      </button>
      {open && (
        <div
          className="absolute right-0 top-full mt-1 bg-popover text-popover-foreground border border-border rounded-lg shadow-lg py-1 z-50 min-w-[180px]"
          onPointerDown={stopLabelBubbling}
          onClick={stopLabelBubbling}
        >
          {canRemove && (
            <button
              type="button"
              onClick={(e) => {
                e.preventDefault();
                stopLabelBubbling(e);
                setOpen(false);
                onRemove();
              }}
              className="flex items-center gap-2 w-full px-3 py-1.5 text-sm text-destructive hover:bg-destructive/10 transition-colors cursor-pointer text-left"
            >
              <Trash2 className="size-3.5 shrink-0" />
              Remove
            </button>
          )}
          <button
            type="button"
            onClick={(e) => {
              e.preventDefault();
              stopLabelBubbling(e);
              setOpen(false);
              onShowInFolder();
            }}
            className="flex items-center gap-2 w-full px-3 py-1.5 text-sm hover:bg-muted transition-colors cursor-pointer text-left"
          >
            <FolderOpen className="size-3.5 shrink-0 opacity-70" />
            {revealInFolderMenuLabel()}
          </button>
        </div>
      )}
    </div>
  );
}

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
  const [llmProgress, setLlmProgress] = useState<DownloadProgress | null>(null);
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

  async function handleSelectDownloadedModel(modelId: string) {
    if (modelId === currentModelId || downloading) return;
    if (!(downloadStatus[modelId] ?? false)) return;
    try {
      await invoke("set_selected_model", { modelId });
      setCurrentModelId(modelId);
      setError(null);
    } catch (err) {
      const e = err as { message?: string };
      setError(e.message ?? "Failed to switch model.");
    }
  }

  async function handleDownloadWhisper(modelId: string) {
    if (downloading) return;
    setDownloading(modelId);
    setError(null);
    setProgress(null);
    try {
      await invoke("download_model", { modelId });
      setDownloadStatus((prev) => ({ ...prev, [modelId]: true }));
    } catch (err) {
      const e = err as { message?: string };
      setError(e.message ?? "Download failed. Please try again.");
    } finally {
      setDownloading(null);
      setProgress(null);
    }
  }

  async function handleRemoveWhisperModel(modelId: string) {
    const confirmed = await ask(
      "This deletes the model file from your computer. If this was the active model, choose and download another before transcribing.",
      {
        title: "Remove downloaded model?",
        kind: "warning",
      },
    );
    if (!confirmed) return;
    try {
      await invoke("remove_model", { modelId });
      setDownloadStatus((prev) => ({ ...prev, [modelId]: false }));
      if (currentModelId === modelId) {
        setCurrentModelId(null);
      }
      setError(null);
    } catch (err) {
      const e = err as { message?: string };
      setError(e.message ?? "Failed to remove model.");
    }
  }

  async function handleRemoveLlmModel(modelId: string) {
    const confirmed = await ask(
      "This deletes the model file from your computer. If this was the active model, choose and download another before summarizing.",
      {
        title: "Remove downloaded model?",
        kind: "warning",
      },
    );
    if (!confirmed) return;
    try {
      await invoke("remove_llm_model", { modelId });
      setLlmDownloadStatus((prev) => ({ ...prev, [modelId]: false }));
      if (selectedLlmModelId === modelId) {
        setSelectedLlmModelId(null);
      }
      setLlmError(null);
    } catch (err) {
      const e = err as { message?: string };
      setLlmError(e.message ?? "Failed to remove model.");
    }
  }

  async function handleShowWhisperInFolder(modelId: string) {
    try {
      const path = await invoke<string | null>("get_whisper_model_file_path", {
        modelId,
      });
      if (!path) {
        setError("Model file not found on disk.");
        return;
      }
      await revealItemInDir(path);
      setError(null);
    } catch (err) {
      const e = err as { message?: string };
      setError(e.message ?? "Could not show file in folder.");
    }
  }

  async function handleShowLlmInFolder(modelId: string) {
    try {
      const path = await invoke<string | null>("get_llm_model_file_path", {
        modelId,
      });
      if (!path) {
        setLlmError("Model file not found on disk.");
        return;
      }
      await revealItemInDir(path);
      setLlmError(null);
    } catch (err) {
      const e = err as { message?: string };
      setLlmError(e.message ?? "Could not show file in folder.");
    }
  }

  async function handleSelectDownloadedLlmModel(modelId: string) {
    if (modelId === selectedLlmModelId || llmDownloading) return;
    if (!(llmDownloadStatus[modelId] ?? false)) return;
    try {
      await invoke("set_selected_llm_model", { modelId });
      setSelectedLlmModelId(modelId);
      setLlmError(null);
    } catch (err) {
      const e = err as { message?: string };
      setLlmError(e.message ?? "Failed to switch model.");
    }
  }

  async function handleDownloadLlm(modelId: string) {
    if (llmDownloading) return;
    setLlmDownloading(modelId);
    setLlmError(null);
    setLlmProgress(null);
    try {
      await invoke("download_llm_model", { modelId });
      setLlmDownloadStatus((prev) => ({ ...prev, [modelId]: true }));
    } catch (err) {
      const e = err as { message?: string };
      setLlmError(e.message ?? "Download failed. Please try again.");
    } finally {
      setLlmDownloading(null);
      setLlmProgress(null);
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

          {error && !downloading && (
            <p className="text-sm text-destructive mb-2">{error}</p>
          )}

          {loading ? (
            <div className="flex flex-col gap-3">
              <Skeleton className="h-20 rounded-lg" />
              <Skeleton className="h-20 rounded-lg" />
            </div>
          ) : (
            <RadioGroup
              value={currentModelId ?? undefined}
              onValueChange={(id) => void handleSelectDownloadedModel(id)}
              disabled={!!downloading}
              className="flex flex-col gap-2 w-full"
            >
              {models.map((model) => {
                const isDownloaded = downloadStatus[model.id] ?? false;
                const isDownloading = downloading === model.id;

                if (!isDownloaded) {
                  return (
                    <div
                      key={model.id}
                      className="w-full rounded-lg border border-border p-2.5"
                    >
                      <Field orientation="horizontal">
                        <RadioGroupItem value={model.id} className="mt-0.5" />
                        <FieldContent className="min-w-0">
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
                          </FieldDescription>
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
                        </FieldContent>
                        {!isDownloading && (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="shrink-0"
                            disabled={!!downloading}
                            onClick={(e) => {
                              e.preventDefault();
                              e.stopPropagation();
                              void handleDownloadWhisper(model.id);
                            }}
                          >
                            <Download className="size-3.5" />
                            Download
                          </Button>
                        )}
                      </Field>
                    </div>
                  );
                }

                const radioId = `model-${model.id}`;
                return (
                  <FieldLabel
                    key={model.id}
                    htmlFor={radioId}
                    className="w-full"
                  >
                    <Field orientation="horizontal" className="items-start">
                      <RadioGroupItem
                        value={model.id}
                        id={radioId}
                        className="mt-0.5"
                      />
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
                        </FieldDescription>
                      </FieldContent>
                      <ModelActionsMenu
                        canRemove={currentModelId !== model.id}
                        onShowInFolder={() =>
                          void handleShowWhisperInFolder(model.id)
                        }
                        onRemove={() => void handleRemoveWhisperModel(model.id)}
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

          {llmError && !llmDownloading && (
            <p className="text-sm text-destructive mb-2">{llmError}</p>
          )}

          {loading ? (
            <div className="flex flex-col gap-3">
              <Skeleton className="h-20 rounded-lg" />
              <Skeleton className="h-20 rounded-lg" />
            </div>
          ) : (
            <RadioGroup
              value={selectedLlmModelId ?? undefined}
              onValueChange={(id) => void handleSelectDownloadedLlmModel(id)}
              disabled={!!llmDownloading}
              className="flex flex-col gap-2 w-full"
            >
              {llmModels.map((model) => {
                const isDownloaded = llmDownloadStatus[model.id] ?? false;
                const isDownloading = llmDownloading === model.id;

                if (!isDownloaded) {
                  return (
                    <div
                      key={model.id}
                      className="w-full rounded-lg border border-border p-2.5"
                    >
                      <Field orientation="horizontal">
                        <RadioGroupItem value={model.id} className="mt-0.5" />
                        <FieldContent className="min-w-0">
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
                        </FieldContent>
                        {!isDownloading && (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="shrink-0"
                            disabled={!!llmDownloading}
                            onClick={(e) => {
                              e.preventDefault();
                              e.stopPropagation();
                              void handleDownloadLlm(model.id);
                            }}
                          >
                            <Download className="size-3.5" />
                            Download
                          </Button>
                        )}
                      </Field>
                    </div>
                  );
                }

                const llmRadioId = `llm-model-${model.id}`;
                return (
                  <FieldLabel
                    key={model.id}
                    htmlFor={llmRadioId}
                    className="w-full"
                  >
                    <Field orientation="horizontal" className="items-start">
                      <RadioGroupItem
                        value={model.id}
                        id={llmRadioId}
                        className="mt-0.5"
                      />
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
                        </FieldDescription>
                      </FieldContent>
                      <ModelActionsMenu
                        canRemove={selectedLlmModelId !== model.id}
                        onShowInFolder={() =>
                          void handleShowLlmInFolder(model.id)
                        }
                        onRemove={() => void handleRemoveLlmModel(model.id)}
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
