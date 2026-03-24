import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  type MeetingRow,
  type MeetingDetail,
  type MeetingTitleInfo,
  type SummarizationQueue,
  type TokenPayload,
  type TitlePayload,
  minutesBodyFromDocuments,
} from "./types";

export function useMeetingSummarization(
  meeting: MeetingRow,
  minutesBody: string | null,
  onTitleChange?: (info: MeetingTitleInfo) => void,
) {
  const [currentTitle, setCurrentTitle] = useState(meeting.title);
  const [generatingTitle, setGeneratingTitle] = useState(false);

  const [summarizing, setSummarizing] = useState(false);
  const [streamedSummary, setStreamedSummary] = useState("");
  const [thinkingText, setThinkingText] = useState("");
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [llmModelReady, setLlmModelReady] = useState<boolean | null>(null);
  const [currentSummary, setCurrentSummary] = useState<string | null>(
    minutesBody,
  );

  const unlistenTokenRef = useRef<(() => void) | null>(null);
  const unlistenThinkingRef = useRef<(() => void) | null>(null);
  const minutesBodyRef = useRef(minutesBody);
  minutesBodyRef.current = minutesBody;

  const registerStreamListeners = useCallback(async () => {
    const tokenUn = await listen<TokenPayload>("summary:token", (event) => {
      if (event.payload.meeting_id !== meeting.id) return;
      setStreamedSummary((prev) => prev + event.payload.token);
    });
    const thinkUn = await listen<TokenPayload>("summary:thinking", (event) => {
      if (event.payload.meeting_id !== meeting.id) return;
      setThinkingText((prev) => prev + event.payload.token);
    });
    unlistenTokenRef.current = tokenUn;
    unlistenThinkingRef.current = thinkUn;
    return () => {
      tokenUn();
      thinkUn();
      unlistenTokenRef.current = null;
      unlistenThinkingRef.current = null;
    };
  }, [meeting.id]);

  const cleanupStreamListeners = useCallback(() => {
    unlistenTokenRef.current?.();
    unlistenThinkingRef.current?.();
    unlistenTokenRef.current = null;
    unlistenThinkingRef.current = null;
  }, []);

  useEffect(() => {
    async function checkLlmModel() {
      try {
        const selected = await invoke<string | null>("get_selected_llm_model");
        if (!selected) {
          setLlmModelReady(false);
          return;
        }
        const ready = await invoke<boolean>("get_llm_model_status", {
          modelId: selected,
        });
        setLlmModelReady(ready);
      } catch {
        setLlmModelReady(false);
      }
    }
    checkLlmModel();
  }, []);

  useEffect(() => {
    setCurrentSummary(minutesBody);
  }, [minutesBody]);

  useEffect(() => {
    setCurrentTitle(meeting.title);
  }, [meeting.title]);

  useEffect(() => {
    onTitleChange?.({
      title: currentTitle,
      generatingTitle,
      createdAt: meeting.created_at,
      durationMs: meeting.duration_ms,
    });
  }, [
    currentTitle,
    generatingTitle,
    meeting.created_at,
    meeting.duration_ms,
    onTitleChange,
  ]);

  useEffect(() => {
    let cancelled = false;
    async function checkActive() {
      try {
        const queue = await invoke<SummarizationQueue>("get_summarization_queue");
        if (cancelled) return;

        const isActive = queue.active === meeting.id;
        const isQueued = queue.pending.includes(meeting.id);

        if (!isActive && !isQueued) return;

        if (isActive) {
          try {
            const fresh = await invoke<MeetingDetail>("get_meeting", {
              meetingId: meeting.id,
            });
            if (cancelled) return;
            const freshMinutes = minutesBodyFromDocuments(fresh.documents);
            if (
              freshMinutes &&
              freshMinutes !== minutesBodyRef.current
            ) {
              setCurrentSummary(freshMinutes);
              setGeneratingTitle(true);
              return;
            }
          } catch {
            // Fall through to default behaviour
          }
        }

        setSummarizing(true);
        setGeneratingTitle(true);

        const cleanup = await registerStreamListeners();
        if (cancelled) {
          cleanup();
          return;
        }
      } catch {
        // Command not available or failed — ignore
      }
    }
    checkActive();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meeting.id]);

  useEffect(() => {
    const unlisten = listen<string>("summary:complete", async (event) => {
      if (event.payload !== meeting.id) return;
      try {
        const result = await invoke<MeetingDetail>("get_meeting", {
          meetingId: meeting.id,
        });
        setCurrentSummary(minutesBodyFromDocuments(result.documents));
      } catch {
        // fall through — summary will appear on next navigation
      }
      cleanupStreamListeners();
      setSummarizing(false);
      setStreamedSummary("");
      setThinkingText("");
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [meeting.id, cleanupStreamListeners]);

  useEffect(() => {
    return () => cleanupStreamListeners();
  }, [cleanupStreamListeners]);

  useEffect(() => {
    const unlisten = listen<TitlePayload>("summary:title", (event) => {
      if (event.payload.meeting_id !== meeting.id) return;
      if (event.payload.title) {
        setCurrentTitle(event.payload.title);
      }
      setGeneratingTitle(false);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [meeting.id]);

  async function handleSummarize() {
    if (currentSummary) {
      const confirmed = await ask("This will replace the existing summary.", {
        title: "Resummarize?",
        kind: "warning",
      });
      if (!confirmed) return;
    }

    setSummarizing(true);
    setGeneratingTitle(true);
    setStreamedSummary("");
    setThinkingText("");
    setSummaryError(null);

    await registerStreamListeners();

    try {
      await invoke<string>("summarize_meeting", {
        meetingId: meeting.id,
      });
    } catch (err) {
      const e = err as { message?: string };
      setSummaryError(e.message ?? "Summarization failed.");
      cleanupStreamListeners();
      setSummarizing(false);
      setGeneratingTitle(false);
    }
  }

  return {
    summarizing,
    streamedSummary,
    thinkingText,
    summaryError,
    llmModelReady,
    currentSummary,
    handleSummarize,
  };
}
