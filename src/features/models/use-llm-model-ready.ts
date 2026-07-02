import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "@/hooks/use-tauri-event";
import type { DownloadProgress } from "./types";

/**
 * Whether the selected LLM model is downloaded and ready (`null` until the
 * first check completes). Re-checks when a model download finishes so the UI
 * flips out of the "download a model" state without a remount; the progress
 * event fires before the atomic rename completes, so it re-polls once on a
 * short delay.
 */
export function useLlmModelReady(): {
  ready: boolean | null;
  recheck: () => Promise<void>;
} {
  const [ready, setReady] = useState<boolean | null>(null);

  const recheck = useCallback(async () => {
    try {
      const selected = await invoke<string | null>("get_selected_llm_model");
      if (!selected) {
        setReady(false);
        return;
      }
      const isReady = await invoke<boolean>("get_llm_model_status", {
        modelId: selected,
      });
      setReady(isReady);
    } catch {
      setReady(false);
    }
  }, []);

  useEffect(() => {
    void recheck();
  }, [recheck]);

  useTauriEvent<DownloadProgress>("llm-model:download-progress", (payload) => {
    const { downloaded_bytes, total_bytes } = payload;
    if (total_bytes <= 0 || downloaded_bytes < total_bytes) return;
    void recheck();
    setTimeout(() => void recheck(), 250);
  });

  return { ready, recheck };
}
