import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { useTauriEvent } from "@/hooks/use-tauri-event";
import { listenBatch } from "@/lib/tauri-events";
import {
  type MeetingRow,
  type MeetingDetail,
  type MeetingTitleInfo,
  type SummarizationQueue,
  type TitlePayload,
  type SummaryPlanPayload,
  type SectionStartPayload,
  type SectionTokenPayload,
  type SectionDonePayload,
  type SummarySection,
  summaryBodyFromDocuments,
} from "./types";
import { useLlmModelReady } from "@/features/models";
import { useSummaryTemplates } from "./use-summary-templates";
import { toastError } from "@/lib/toast";

export function useMeetingSummarization(
  meeting: MeetingRow,
  summaryBody: string | null,
  onTitleChange?: (info: MeetingTitleInfo) => void,
) {
  const [currentTitle, setCurrentTitle] = useState(meeting.title);
  const [generatingTitle, setGeneratingTitle] = useState(false);

  const [summarizing, setSummarizing] = useState(false);
  const [sections, setSections] = useState<SummarySection[]>([]);
  const { ready: llmModelReady } = useLlmModelReady();
  const [currentSummary, setCurrentSummary] = useState<string | null>(
    summaryBody,
  );

  const { templates, selectedTemplate, setSelectedTemplate } =
    useSummaryTemplates(meeting);

  const unlistenStreamRef = useRef<UnlistenFn | null>(null);
  const summaryBodyRef = useRef(summaryBody);
  summaryBodyRef.current = summaryBody;

  const cleanupStreamListeners = useCallback(() => {
    unlistenStreamRef.current?.();
    unlistenStreamRef.current = null;
  }, []);

  // Per-section streaming: the run announces its sections up front (`summary:plan`),
  // then each section streams its body. `summary:section_start` flips a section to
  // "thinking" (its transcript prefill), `summary:token` appends body + flips to
  // "writing", and `summary:section_done` marks it "done" (or "skipped" if empty).
  const registerStreamListeners = useCallback(async () => {
    if (unlistenStreamRef.current) {
      return cleanupStreamListeners;
    }
    unlistenStreamRef.current = await listenBatch([
      listen<SummaryPlanPayload>("summary:plan", (event) => {
        if (event.payload.meeting_id !== meeting.id) return;
        const ordered = [...event.payload.sections].sort(
          (a, b) => a.index - b.index,
        );
        setSections(
          ordered.map(
            (s): SummarySection => ({
              heading: s.heading,
              body: "",
              state: "pending",
            }),
          ),
        );
      }),
      listen<SectionStartPayload>("summary:section_start", (event) => {
        if (event.payload.meeting_id !== meeting.id) return;
        const { index } = event.payload;
        setSections((prev) =>
          prev.map((s, i): SummarySection =>
            i === index && s.state === "pending" ? { ...s, state: "thinking" } : s,
          ),
        );
      }),
      listen<SectionTokenPayload>("summary:token", (event) => {
        if (event.payload.meeting_id !== meeting.id) return;
        const { index, token } = event.payload;
        setSections((prev) =>
          prev.map((s, i): SummarySection =>
            i === index ? { ...s, body: s.body + token, state: "writing" } : s,
          ),
        );
      }),
      listen<SectionDonePayload>("summary:section_done", (event) => {
        if (event.payload.meeting_id !== meeting.id) return;
        const { index, empty } = event.payload;
        setSections((prev) =>
          prev.map((s, i): SummarySection =>
            i === index ? { ...s, state: empty ? "skipped" : "done" } : s,
          ),
        );
      }),
    ]);
    return cleanupStreamListeners;
  }, [meeting.id, cleanupStreamListeners]);

  useEffect(() => {
    setCurrentSummary(summaryBody);
  }, [summaryBody]);

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
            const freshSummary = summaryBodyFromDocuments(fresh.documents);
            if (
              freshSummary &&
              freshSummary !== summaryBodyRef.current
            ) {
              setCurrentSummary(freshSummary);
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

  useTauriEvent<string>("summary:complete", async (completedId) => {
    if (completedId !== meeting.id) return;
    try {
      const result = await invoke<MeetingDetail>("get_meeting", {
        meetingId: meeting.id,
      });
      setCurrentSummary(summaryBodyFromDocuments(result.documents));
    } catch {
      // fall through — summary will appear on next navigation
    }
    cleanupStreamListeners();
    setSummarizing(false);
    setSections([]);
  });

  useEffect(() => {
    return () => cleanupStreamListeners();
  }, [cleanupStreamListeners]);

  useTauriEvent<TitlePayload>("summary:title", (payload) => {
    if (payload.meeting_id !== meeting.id) return;
    if (payload.title) {
      setCurrentTitle(payload.title);
    }
    setGeneratingTitle(false);
  });

  useTauriEvent<string>("summary:started", async (startedId) => {
    if (startedId !== meeting.id) return;
    if (unlistenStreamRef.current) return;
    setSummarizing(true);
    setGeneratingTitle(true);
    setSections([]);
    await registerStreamListeners();
  });

  // Core (re)summarize with an explicit template id — no confirmation. Callers
  // own the confirm step so they can word it for their context.
  async function runSummarize(templateId: string) {
    setSummarizing(true);
    setGeneratingTitle(true);
    setSections([]);

    await registerStreamListeners();

    try {
      await invoke<string>("summarize_meeting", {
        meetingId: meeting.id,
        template: templateId,
      });
    } catch (err) {
      const e = err as { message?: string };
      toastError(e.message ?? "Summarization failed.");
      cleanupStreamListeners();
      setSummarizing(false);
      setGeneratingTitle(false);
    }
  }

  async function handleSummarize() {
    if (currentSummary) {
      const confirmed = await ask("This will replace the existing summary.", {
        title: "Resummarize?",
        kind: "warning",
      });
      if (!confirmed) return;
    }
    await runSummarize(selectedTemplate);
  }

  // Switching the template (re)summarizes with it. If a summary already exists,
  // confirm first; on cancel the selection is left unchanged (the controlled
  // Select reverts). With no summary yet, just remember the choice for the next
  // Summarize click.
  async function handleTemplateChange(next: string) {
    if (next === selectedTemplate) return;
    if (currentSummary) {
      const name = templates.find((t) => t.id === next)?.name ?? next;
      const confirmed = await ask(
        `Resummarize this meeting with the "${name}" template? This replaces the current summary.`,
        { title: "Switch template?", kind: "warning" },
      );
      if (!confirmed) return;
      setSelectedTemplate(next);
      await runSummarize(next);
    } else {
      setSelectedTemplate(next);
    }
  }

  return {
    summarizing,
    sections,
    llmModelReady,
    currentSummary,
    handleSummarize,
    templates,
    selectedTemplate,
    handleTemplateChange,
  };
}
