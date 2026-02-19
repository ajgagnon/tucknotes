import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface ModelSetupProps {
  onComplete: () => void;
}

interface ModelInfo {
  id: string;
  name: string;
  description: string;
  size_bytes: number;
  filename: string;
}

interface DownloadProgress {
  model_id: string;
  downloaded_bytes: number;
  total_bytes: number;
}

function formatSize(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(0)} MB`;
  return `${(bytes / 1_000).toFixed(0)} KB`;
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
      if (list.length > 0) setSelectedId(list[0].id);
    });
  }, []);

  useEffect(() => {
    if (!downloading) return;
    const unlisten = listen<DownloadProgress>(
      "model:download-progress",
      (event) => {
        setProgress(event.payload);
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [downloading]);

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
      <div className="min-h-screen flex items-center justify-center p-8">
        <div className="max-w-[460px] w-full text-center">
          <h1 className="text-2xl font-bold mb-2">Downloading model</h1>
          <p className="text-neutral-500 dark:text-neutral-400 text-[0.95rem] mb-7 leading-relaxed">
            {selectedModel?.name ?? "Model"} —{" "}
            {formatSize(selectedModel?.size_bytes ?? 0)}
          </p>

          <div className="w-full bg-neutral-200 dark:bg-neutral-700 rounded-full h-3 mb-3 overflow-hidden">
            <div
              className="bg-primary h-full rounded-full transition-all duration-300 ease-out"
              style={{ width: `${progressPercent}%` }}
            />
          </div>

          <p className="text-sm text-neutral-500 dark:text-neutral-400 tabular-nums">
            {progress
              ? `${formatSize(progress.downloaded_bytes)} / ${formatSize(progress.total_bytes)}`
              : "Starting download…"}
          </p>

          {error && (
            <div className="mt-5">
              <p className="text-sm text-danger mb-3">{error}</p>
              <button
                className="border-[1.5px] border-primary dark:border-blue-400 text-primary dark:text-blue-400 bg-transparent rounded-xl py-2 px-6 text-sm font-semibold cursor-pointer transition-all duration-200 hover:bg-primary/8 dark:hover:bg-blue-400/10"
                onClick={handleDownload}
              >
                Retry
              </button>
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen flex items-center justify-center p-8">
      <div className="max-w-[460px] w-full text-center">
        <h1 className="text-2xl font-bold mb-2">
          Choose a transcription model
        </h1>
        <p className="text-neutral-500 dark:text-neutral-400 text-[0.95rem] mb-7 leading-relaxed">
          Select a Whisper model to power your meeting notes. You can change
          this later in settings.
        </p>

        <div className="flex flex-col gap-3 mb-6">
          {models.map((model) => {
            const isSelected = selectedId === model.id;
            return (
              <button
                key={model.id}
                onClick={() => setSelectedId(model.id)}
                className={`rounded-xl p-5 px-6 text-left transition-all duration-200 cursor-pointer ${
                  isSelected
                    ? "border-2 border-primary bg-primary/5 dark:bg-primary/10"
                    : "border border-black/8 bg-black/3 dark:bg-white/5 dark:border-white/10 hover:border-black/15 dark:hover:border-white/20"
                }`}
              >
                <div className="flex items-center justify-between mb-2">
                  <h3 className="text-[0.95rem] font-semibold">{model.name}</h3>
                  <div
                    className={`w-5 h-5 rounded-full border-2 flex items-center justify-center shrink-0 transition-colors ${
                      isSelected
                        ? "border-primary bg-primary"
                        : "border-neutral-300 dark:border-neutral-600"
                    }`}
                  >
                    {isSelected && (
                      <div className="w-2 h-2 rounded-full bg-white" />
                    )}
                  </div>
                </div>
                <p className="text-sm text-neutral-600 dark:text-neutral-400 leading-relaxed mb-2">
                  {model.description}
                </p>
                <span className="text-xs font-medium text-neutral-400 dark:text-neutral-500">
                  {formatSize(model.size_bytes)}
                </span>
              </button>
            );
          })}
        </div>

        <button
          className="w-full border-none rounded-xl py-3 px-8 text-[0.95rem] font-semibold cursor-pointer bg-primary text-white shadow-[0_2px_8px_rgba(67,97,238,0.25)] transition-all duration-200 hover:bg-primary-hover hover:shadow-[0_4px_12px_rgba(67,97,238,0.35)] hover:-translate-y-px active:translate-y-0 disabled:opacity-50 disabled:cursor-default disabled:hover:translate-y-0 disabled:hover:shadow-[0_2px_8px_rgba(67,97,238,0.25)]"
          onClick={handleDownload}
          disabled={!selectedId}
        >
          Download & Continue
        </button>
      </div>
    </div>
  );
}

export default ModelSetup;
