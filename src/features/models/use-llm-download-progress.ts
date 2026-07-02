import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "@/hooks/use-tauri-event";
import type { DownloadProgress, ModelInfo } from "./types";

export interface LlmDownloadStatus {
  modelId: string;
  name: string;
  downloadedBytes: number;
  totalBytes: number;
  percent: number;
  done: boolean;
}

const COMPLETION_LINGER_MS = 2500;

/// Subscribes to llm-model:download-progress globally so a sidebar indicator
/// can reflect background downloads kicked off from onboarding or settings.
/// Returns null when nothing is happening; lingers briefly after completion
/// so the user sees the download finished.
export function useLlmDownloadProgress(): LlmDownloadStatus | null {
  const [status, setStatus] = useState<LlmDownloadStatus | null>(null);
  const namesRef = useRef<Map<string, string>>(new Map());
  const lingerTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<ModelInfo[]>("list_available_llm_models").then((list) => {
      if (cancelled) return;
      const map = new Map<string, string>();
      for (const m of list) map.set(m.id, m.name);
      namesRef.current = map;
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useTauriEvent<DownloadProgress>("llm-model:download-progress", (p) => {
    const total = p.total_bytes;
    const downloaded = p.downloaded_bytes;
    const percent = total > 0 ? Math.min((downloaded / total) * 100, 100) : 0;
    const done = total > 0 && downloaded >= total;

    if (lingerTimerRef.current) {
      clearTimeout(lingerTimerRef.current);
      lingerTimerRef.current = null;
    }

    setStatus({
      modelId: p.model_id,
      name: namesRef.current.get(p.model_id) ?? "Summarization model",
      downloadedBytes: downloaded,
      totalBytes: total,
      percent,
      done,
    });

    if (done) {
      lingerTimerRef.current = setTimeout(() => {
        setStatus(null);
        lingerTimerRef.current = null;
      }, COMPLETION_LINGER_MS);
    }
  });

  useEffect(() => {
    return () => {
      if (lingerTimerRef.current) {
        clearTimeout(lingerTimerRef.current);
        lingerTimerRef.current = null;
      }
    };
  }, []);

  return status;
}
