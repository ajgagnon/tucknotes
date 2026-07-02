import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "@/hooks/use-tauri-event";
import type { SummarizationQueue } from "@/features/meetings/types";

export function useSummarizationActive(): boolean {
  const [active, setActive] = useState(false);
  const mountedRef = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const q = await invoke<SummarizationQueue>("get_summarization_queue");
      if (mountedRef.current) {
        setActive(q.active != null || q.pending.length > 0);
      }
    } catch {
      /* ignore */
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    void refresh();
    return () => {
      mountedRef.current = false;
    };
  }, [refresh]);

  useTauriEvent("summary:started", () => void refresh());
  useTauriEvent("summary:complete", () => void refresh());

  return active;
}
