import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { SummarizationQueue } from "@/features/meetings/types";

export function useSummarizationActive(): boolean {
  const [active, setActive] = useState(false);

  useEffect(() => {
    let cancelled = false;
    async function refresh() {
      try {
        const q = await invoke<SummarizationQueue>("get_summarization_queue");
        if (!cancelled) setActive(q.active != null || q.pending.length > 0);
      } catch {
        /* ignore */
      }
    }
    refresh();

    const unStart = listen<string>("summary:started", () => refresh());
    const unDone = listen<string>("summary:complete", () => refresh());
    return () => {
      cancelled = true;
      unStart.then((fn) => fn());
      unDone.then((fn) => fn());
    };
  }, []);

  return active;
}
