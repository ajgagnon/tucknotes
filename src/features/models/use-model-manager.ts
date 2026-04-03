import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { ModelInfo, DownloadProgress } from "./types";

export interface ModelManagerConfig {
  listCommand: string;
  statusCommand: string;
  getSelectedCommand: string;
  setSelectedCommand: string;
  downloadCommand: string;
  removeCommand: string;
  getFilePathCommand: string;
  progressEvent: string;
  removeConfirmMessage: string;
}

export const WHISPER_MODEL_CONFIG: ModelManagerConfig = {
  listCommand: "list_available_models",
  statusCommand: "get_model_status",
  getSelectedCommand: "get_selected_model",
  setSelectedCommand: "set_selected_model",
  downloadCommand: "download_model",
  removeCommand: "remove_model",
  getFilePathCommand: "get_whisper_model_file_path",
  progressEvent: "model:download-progress",
  removeConfirmMessage:
    "This deletes the model file from your computer. If this was the active model, choose and download another before transcribing.",
};

export const LLM_MODEL_CONFIG: ModelManagerConfig = {
  listCommand: "list_available_llm_models",
  statusCommand: "get_llm_model_status",
  getSelectedCommand: "get_selected_llm_model",
  setSelectedCommand: "set_selected_llm_model",
  downloadCommand: "download_llm_model",
  removeCommand: "remove_llm_model",
  getFilePathCommand: "get_llm_model_file_path",
  progressEvent: "llm-model:download-progress",
  removeConfirmMessage:
    "This deletes the model file from your computer. If this was the active model, choose and download another before summarizing.",
};

function errorMessage(err: unknown, fallback: string): string {
  const e = err as { message?: string };
  return e.message ?? fallback;
}

export function useModelManager(config: ModelManagerConfig) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
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
          invoke<ModelInfo[]>(config.listCommand),
          invoke<string | null>(config.getSelectedCommand),
        ]);
        setModels(list);
        setSelectedId(selected);

        const statuses: Record<string, boolean> = {};
        await Promise.all(
          list.map(async (m) => {
            statuses[m.id] = await invoke<boolean>(config.statusCommand, {
              modelId: m.id,
            });
          }),
        );
        setDownloadStatus(statuses);
      } finally {
        setLoading(false);
      }
    }
    void load();
  }, [config]);

  useEffect(() => {
    if (!downloading) return;
    const unlisten = listen<DownloadProgress>(config.progressEvent, (event) =>
      setProgress(event.payload),
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [downloading, config.progressEvent]);

  const selectModel = useCallback(
    async (modelId: string) => {
      if (modelId === selectedId || downloading) return;
      if (!(downloadStatus[modelId] ?? false)) return;
      try {
        await invoke(config.setSelectedCommand, { modelId });
        setSelectedId(modelId);
        setError(null);
      } catch (err) {
        setError(errorMessage(err, "Failed to switch model."));
      }
    },
    [
      selectedId,
      downloading,
      downloadStatus,
      config.setSelectedCommand,
    ],
  );

  const downloadModel = useCallback(
    async (modelId: string) => {
      if (downloading) return;
      setDownloading(modelId);
      setError(null);
      setProgress(null);
      try {
        await invoke(config.downloadCommand, { modelId });
        setDownloadStatus((prev) => ({ ...prev, [modelId]: true }));
      } catch (err) {
        setError(errorMessage(err, "Download failed. Please try again."));
      } finally {
        setDownloading(null);
        setProgress(null);
      }
    },
    [downloading, config.downloadCommand],
  );

  const removeModel = useCallback(
    async (modelId: string) => {
      const confirmed = await ask(config.removeConfirmMessage, {
        title: "Remove downloaded model?",
        kind: "warning",
      });
      if (!confirmed) return;
      try {
        await invoke(config.removeCommand, { modelId });
        setDownloadStatus((prev) => ({ ...prev, [modelId]: false }));
        if (selectedId === modelId) {
          setSelectedId(null);
        }
        setError(null);
      } catch (err) {
        setError(errorMessage(err, "Failed to remove model."));
      }
    },
    [selectedId, config.removeCommand, config.removeConfirmMessage],
  );

  const showInFolder = useCallback(
    async (modelId: string) => {
      try {
        const path = await invoke<string | null>(config.getFilePathCommand, {
          modelId,
        });
        if (!path) {
          setError("Model file not found on disk.");
          return;
        }
        await revealItemInDir(path);
        setError(null);
      } catch (err) {
        setError(errorMessage(err, "Could not show file in folder."));
      }
    },
    [config.getFilePathCommand],
  );

  return {
    models,
    selectedId,
    downloadStatus,
    loading,
    downloading,
    progress,
    error,
    selectModel,
    downloadModel,
    removeModel,
    showInFolder,
  };
}
