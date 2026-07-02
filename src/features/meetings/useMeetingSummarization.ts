import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  type MeetingRow,
  type MeetingDetail,
  type MeetingTitleInfo,
  type SummarizationQueue,
  type TemplateInfo,
  type TitlePayload,
  type SummaryPlanPayload,
  type SectionStartPayload,
  type SectionTokenPayload,
  type SectionDonePayload,
  type SummarySection,
  summaryBodyFromDocuments,
} from "./types";
import type { DownloadProgress } from "@/features/models";
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
  const [llmModelReady, setLlmModelReady] = useState<boolean | null>(null);
  const [currentSummary, setCurrentSummary] = useState<string | null>(
    summaryBody,
  );

  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [selectedTemplate, setSelectedTemplate] = useState<string>(
    meeting.template ?? "default",
  );

  const unlistenStreamRef = useRef<Array<() => void>>([]);
  const summaryBodyRef = useRef(summaryBody);
  summaryBodyRef.current = summaryBody;

  const cleanupStreamListeners = useCallback(() => {
    for (const unlisten of unlistenStreamRef.current) unlisten();
    unlistenStreamRef.current = [];
  }, []);

  // Per-section streaming: the run announces its sections up front (`summary:plan`),
  // then each section streams its body. `summary:section_start` flips a section to
  // "thinking" (its transcript prefill), `summary:token` appends body + flips to
  // "writing", and `summary:section_done` marks it "done" (or "skipped" if empty).
  const registerStreamListeners = useCallback(async () => {
    if (unlistenStreamRef.current.length) {
      return cleanupStreamListeners;
    }
    const planUn = await listen<SummaryPlanPayload>("summary:plan", (event) => {
      if (event.payload.meeting_id !== meeting.id) return;
      const ordered = [...event.payload.sections].sort((a, b) => a.index - b.index);
      setSections(
        ordered.map(
          (s): SummarySection => ({ heading: s.heading, body: "", state: "pending" }),
        ),
      );
    });
    const startUn = await listen<SectionStartPayload>(
      "summary:section_start",
      (event) => {
        if (event.payload.meeting_id !== meeting.id) return;
        const { index } = event.payload;
        setSections((prev) =>
          prev.map((s, i): SummarySection =>
            i === index && s.state === "pending" ? { ...s, state: "thinking" } : s,
          ),
        );
      },
    );
    const tokenUn = await listen<SectionTokenPayload>("summary:token", (event) => {
      if (event.payload.meeting_id !== meeting.id) return;
      const { index, token } = event.payload;
      setSections((prev) =>
        prev.map((s, i): SummarySection =>
          i === index ? { ...s, body: s.body + token, state: "writing" } : s,
        ),
      );
    });
    const doneUn = await listen<SectionDonePayload>(
      "summary:section_done",
      (event) => {
        if (event.payload.meeting_id !== meeting.id) return;
        const { index, empty } = event.payload;
        setSections((prev) =>
          prev.map((s, i): SummarySection =>
            i === index ? { ...s, state: empty ? "skipped" : "done" } : s,
          ),
        );
      },
    );
    unlistenStreamRef.current = [planUn, startUn, tokenUn, doneUn];
    return cleanupStreamListeners;
  }, [meeting.id, cleanupStreamListeners]);

  const checkLlmModel = useCallback(async () => {
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
  }, []);

  useEffect(() => {
    void checkLlmModel();
  }, [checkLlmModel]);

  // Load the (static) list of built-in templates once.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const list = await invoke<TemplateInfo[]>("list_summary_templates");
        if (!cancelled) setTemplates(list);
      } catch {
        // Picker just renders no options; Summarize still works (→ Default).
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Resolve the picker selection whenever the meeting changes: the meeting's
  // stored template wins, then the app-wide default, then "default". Switching
  // the picker afterwards only mutates local state (applied on Summarize).
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      if (meeting.template) {
        setSelectedTemplate(meeting.template);
        return;
      }
      try {
        const appDefault = await invoke<string | null>("get_default_template");
        if (!cancelled) setSelectedTemplate(appDefault ?? "default");
      } catch {
        if (!cancelled) setSelectedTemplate("default");
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meeting.id, meeting.template]);

  // Re-check model readiness when an LLM download finishes so the UI flips
  // out of the "Download a summarization model in Settings…" state without a
  // remount. The progress event fires before the atomic rename completes, so
  // checkLlmModel may briefly still see false; re-poll once on a short delay.
  useEffect(() => {
    const unlisten = listen<DownloadProgress>(
      "llm-model:download-progress",
      (event) => {
        const { downloaded_bytes, total_bytes } = event.payload;
        if (total_bytes <= 0 || downloaded_bytes < total_bytes) return;
        void checkLlmModel();
        setTimeout(() => void checkLlmModel(), 250);
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [checkLlmModel]);

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

  useEffect(() => {
    const unlisten = listen<string>("summary:complete", async (event) => {
      if (event.payload !== meeting.id) return;
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

  useEffect(() => {
    let cancelled = false;
    const unlisten = listen<string>("summary:started", async (event) => {
      if (event.payload !== meeting.id) return;
      if (cancelled) return;
      if (unlistenStreamRef.current.length) return;
      setSummarizing(true);
      setGeneratingTitle(true);
      setSections([]);
      await registerStreamListeners();
    });
    return () => {
      cancelled = true;
      unlisten.then((fn) => fn());
    };
  }, [meeting.id, registerStreamListeners]);

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
