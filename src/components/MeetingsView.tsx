import {
  useState,
  useEffect,
  useRef,
  useCallback,
  type RefObject,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { Sparkles } from "lucide-react";
import ReactMarkdown from "react-markdown";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  useRecording,
  type TranscriptSegment,
  type AppError,
} from "@/hooks/useRecording";
import { AnimatedShinyText } from "@/components/ui/animated-shiny-text";
import { Button } from "./ui/button";

export interface MeetingRow {
  id: string;
  title: string | null;
  created_at: number;
  ended_at: number | null;
  duration_ms: number | null;
  summary: string | null;
}

interface SegmentRow {
  id: number;
  meeting_id: string;
  text: string;
  source: string;
  timestamp_ms: number;
  prompt: string | null;
  created_at: number;
}

export interface MeetingDetail {
  meeting: MeetingRow;
  segments: SegmentRow[];
}

export interface MeetingTitleInfo {
  title: string | null;
  generatingTitle: boolean;
  createdAt: number;
  durationMs: number | null;
}

interface MeetingsViewProps {
  meetingId: string;
  onTitleChange?: (info: MeetingTitleInfo) => void;
}

// Event payload types matching the Rust structs
interface TokenPayload {
  meeting_id: string;
  token: string;
}

interface TitlePayload {
  meeting_id: string;
  title: string;
}

export interface SummarizationQueue {
  active: string | null;
  pending: string[];
}

// ---------------------------------------------------------------------------
// Collapsible thinking block with auto-scroll
// ---------------------------------------------------------------------------

function ThinkingBlock({ text }: { text: string }) {
  const lastLine = text.trimEnd().split("\n").at(-1) ?? "";

  return (
    <div className="h-5 overflow-hidden w-0 min-w-full text-sm text-muted-foreground italic">
      <AnimatedShinyText className="truncate m-0">{lastLine}</AnimatedShinyText>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Transcript segment rendering (used in both live and persisted views)
// ---------------------------------------------------------------------------

function SegmentBubble({
  source,
  text,
  provisional,
}: {
  source: string;
  text: string;
  provisional?: boolean;
}) {
  return (
    <div className={`flex flex-col gap-0.5 ${provisional ? "opacity-50" : ""}`}>
      <span
        className={`text-[0.65rem] font-semibold uppercase tracking-wider ${
          source === "system" ? "text-primary" : "text-success"
        }`}
      >
        {source === "system" ? "Them" : "You"}
      </span>
      <p
        className={`text-sm text-neutral-700 dark:text-neutral-300 m-0 leading-snug ${provisional ? "italic" : ""}`}
      >
        {text}
      </p>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Transcript sub-components
// ---------------------------------------------------------------------------

function LiveTranscript({
  segments,
  provisional,
  scrollRef,
}: {
  segments: TranscriptSegment[];
  provisional: Record<string, TranscriptSegment>;
  scrollRef: RefObject<HTMLDivElement | null>;
}) {
  const hasContent = segments.length > 0 || Object.keys(provisional).length > 0;

  if (!hasContent) {
    return (
      <p className="text-sm text-neutral-400 text-center mt-8">
        Transcript will appear here...
      </p>
    );
  }

  return (
    <>
      {segments.map((seg, i) => (
        <SegmentBubble key={i} source={seg.source} text={seg.text} />
      ))}
      {Object.values(provisional).map((seg) => (
        <SegmentBubble
          key={`provisional-${seg.source}`}
          source={seg.source}
          text={seg.text}
          provisional
        />
      ))}
      <div ref={scrollRef} />
    </>
  );
}

function PersistedTranscript({ segments }: { segments: SegmentRow[] }) {
  if (segments.length === 0) {
    return (
      <p className="text-sm text-neutral-400 text-center mt-8">
        No transcript segments recorded.
      </p>
    );
  }

  return (
    <>
      {segments.map((seg) => (
        <SegmentBubble key={seg.id} source={seg.source} text={seg.text} />
      ))}
    </>
  );
}

// ---------------------------------------------------------------------------
// Meeting detail view (live recording + persisted transcript)
// ---------------------------------------------------------------------------

interface MeetingDetailProps {
  detail: MeetingDetail;
  isLiveRecording: boolean;
  liveSegments: TranscriptSegment[];
  provisional: Record<string, TranscriptSegment>;
  error: AppError | null;
  onTitleChange?: (info: MeetingTitleInfo) => void;
}

function MeetingDetailView({
  detail,
  isLiveRecording,
  liveSegments,
  provisional,
  error,
  onTitleChange,
}: MeetingDetailProps) {
  const transcriptEndRef = useRef<HTMLDivElement>(null);

  // Title state (kept here for summarization context, reported up via callback)
  const [currentTitle, setCurrentTitle] = useState(detail.meeting.title);
  const [generatingTitle, setGeneratingTitle] = useState(false);

  // Summarization state
  const [summarizing, setSummarizing] = useState(false);
  const [streamedSummary, setStreamedSummary] = useState("");
  const [thinkingText, setThinkingText] = useState("");
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [llmModelReady, setLlmModelReady] = useState<boolean | null>(null);
  const [currentSummary, setCurrentSummary] = useState<string | null>(
    detail.meeting.summary,
  );

  // Refs for streaming listener cleanup on unmount
  const unlistenTokenRef = useRef<(() => void) | null>(null);
  const unlistenThinkingRef = useRef<(() => void) | null>(null);

  // Register token + thinking streaming listeners filtered by meeting ID.
  // Returns cleanup function. Stores listeners in refs for cross-effect cleanup.
  async function registerStreamListeners() {
    const tokenUn = await listen<TokenPayload>("summary:token", (event) => {
      if (event.payload.meeting_id !== detail.meeting.id) return;
      setStreamedSummary((prev) => prev + event.payload.token);
    });
    const thinkUn = await listen<TokenPayload>("summary:thinking", (event) => {
      if (event.payload.meeting_id !== detail.meeting.id) return;
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
  }

  function cleanupStreamListeners() {
    unlistenTokenRef.current?.();
    unlistenThinkingRef.current?.();
    unlistenTokenRef.current = null;
    unlistenThinkingRef.current = null;
  }

  // Check if LLM model is available
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

  // Sync when detail changes (e.g. after reload)
  useEffect(() => {
    setCurrentSummary(detail.meeting.summary);
  }, [detail.meeting.summary]);
  useEffect(() => {
    setCurrentTitle(detail.meeting.title);
  }, [detail.meeting.title]);

  // Report title info to parent whenever it changes
  useEffect(() => {
    onTitleChange?.({
      title: currentTitle,
      generatingTitle,
      createdAt: detail.meeting.created_at,
      durationMs: detail.meeting.duration_ms,
    });
  }, [currentTitle, generatingTitle, detail.meeting.created_at, detail.meeting.duration_ms, onTitleChange]);

  // On mount, check if our meeting is active or queued. If so, restore
  // summarizing state and register streaming listeners (they filter by
  // meeting_id, so they're safe to register even while queued — they'll
  // just start receiving tokens when the meeting becomes active).
  useEffect(() => {
    let cancelled = false;
    async function checkActive() {
      try {
        const queue = await invoke<SummarizationQueue>("get_summarization_queue");
        if (cancelled) return;

        const isActive = queue.active === detail.meeting.id;
        const isQueued = queue.pending.includes(detail.meeting.id);

        if (!isActive && !isQueued) return;

        // If this meeting is active (not just queued), the backend may be
        // in one of two phases: summarization or title-gen. The summary is
        // persisted to DB before title gen starts, so a *new* summary in
        // the DB (different from our prop snapshot) means summarization is
        // done and only title gen is still running.
        if (isActive) {
          try {
            const fresh = await invoke<MeetingDetail>("get_meeting", {
              meetingId: detail.meeting.id,
            });
            if (cancelled) return;
            if (fresh.meeting.summary && fresh.meeting.summary !== detail.meeting.summary) {
              setCurrentSummary(fresh.meeting.summary);
              setGeneratingTitle(true);
              return;
            }
          } catch {
            // Fall through to default behaviour
          }
        }

        setSummarizing(true);
        setGeneratingTitle(true);

        // Register streaming listeners (filtered by meeting ID).
        // Safe to register even when queued — no events arrive until active.
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
  }, [detail.meeting.id]);

  // Listen for summary:complete to know when backend finishes summarization.
  // This handles the case where the user navigated away and came back — the
  // invoke promise is gone, so we rely on this event + DB reload instead.
  useEffect(() => {
    const unlisten = listen<string>("summary:complete", async (event) => {
      if (event.payload !== detail.meeting.id) return;
      // Reload meeting from DB to get the persisted summary
      try {
        const result = await invoke<MeetingDetail>("get_meeting", {
          meetingId: detail.meeting.id,
        });
        setCurrentSummary(result.meeting.summary);
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detail.meeting.id]);

  // Clean up streaming listeners on unmount (guards against listener leak
  // when user navigates away during active summarization)
  useEffect(() => {
    return () => cleanupStreamListeners();
  }, []);

  // Listen for AI-generated title (fires after summary, from background task)
  useEffect(() => {
    const unlisten = listen<TitlePayload>("summary:title", (event) => {
      if (event.payload.meeting_id !== detail.meeting.id) return;
      if (event.payload.title) {
        setCurrentTitle(event.payload.title);
      }
      setGeneratingTitle(false);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [detail.meeting.id]);

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

    // Register streaming listeners up front (filtered by meeting ID).
    // Safe for both "started" and "queued" — no events arrive until active.
    await registerStreamListeners();

    try {
      await invoke<string>("summarize_meeting", {
        meetingId: detail.meeting.id,
      });
    } catch (err) {
      const e = err as { message?: string };
      setSummaryError(e.message ?? "Summarization failed.");
      cleanupStreamListeners();
      setSummarizing(false);
      setGeneratingTitle(false);
    }
    // Note: cleanup of streaming listeners happens in the summary:complete
    // listener effect, not here. The invoke returns immediately while
    // summarization continues in the background.
  }

  // Auto-scroll for live transcript
  useEffect(() => {
    if (isLiveRecording) {
      transcriptEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [liveSegments, provisional, isLiveRecording]);

  return (
    <div className="flex flex-col h-full p-5">
      {/* Error display */}
      {error && (
        <div className="bg-red-50 border border-red-200 text-red-700 rounded-lg py-3 px-4 text-sm mb-4 text-center dark:bg-danger/10 dark:border-danger/25 dark:text-red-300">
          {error.kind === "PermissionDenied" ? (
            <>
              <p className="m-0 mb-2 font-medium">
                Permission needed to capture audio
              </p>
              <p className="m-0 mb-3 text-xs text-red-500 dark:text-red-400">
                Enable Screen Recording in macOS settings to get started.
              </p>
              <button
                className="border-[1.5px] border-red-300 dark:border-red-400/50 text-red-700 dark:text-red-300 bg-transparent rounded-lg py-1.5 px-4 text-xs font-semibold cursor-pointer transition-all duration-200 hover:bg-red-100 dark:hover:bg-red-400/10"
                onClick={() => invoke("open_screen_recording_settings")}
              >
                Open System Settings
              </button>
            </>
          ) : (
            error.message
          )}
        </div>
      )}

      {isLiveRecording ? (
        /* During live recording, show transcript directly (no tabs) */
        <div className="flex-1 overflow-y-auto flex flex-col gap-3 ">
          <LiveTranscript
            segments={liveSegments}
            provisional={provisional}
            scrollRef={transcriptEndRef}
          />
        </div>
      ) : (
        /* After recording, show Summary / Transcript tabs */
        <Tabs defaultValue="summary" className="flex flex-col flex-1 min-h-0">
          <div className="flex items-center justify-between">
            <TabsList>
              <TabsTrigger
                value="summary"
                className="flex items-center gap-1.5"
              >
                Summary
                {summarizing && (
                  <span className="inline-block w-1.5 h-1.5 rounded-full bg-muted-foreground animate-pulse" />
                )}
              </TabsTrigger>
              <TabsTrigger value="transcript">Transcript</TabsTrigger>
            </TabsList>
            {!summarizing && llmModelReady && (
              <Button
                onClick={handleSummarize}
                size="xs"
                variant="outline"
                className="rounded-full"
              >
                <Sparkles className="size-2.5" />
                {currentSummary ? "Resummarize" : "Summarize"}
              </Button>
            )}
          </div>

          <TabsContent value="summary" className="flex-1 overflow-y-auto mt-4">
            {summarizing ? (
              <div className="text-sm leading-relaxed">
                {thinkingText && !streamedSummary && (
                  <ThinkingBlock text={thinkingText} />
                )}
                {streamedSummary && (
                  <div className="prose prose-sm dark:prose-invert max-w-none">
                    <ReactMarkdown>{streamedSummary}</ReactMarkdown>
                    <span className="inline-block w-1.5 h-4 bg-primary animate-pulse ml-0.5 align-text-bottom rounded-sm" />
                  </div>
                )}
              </div>
            ) : currentSummary ? (
              <div className="prose prose-sm dark:prose-invert max-w-none">
                <ReactMarkdown>{currentSummary}</ReactMarkdown>
              </div>
            ) : llmModelReady === false ? (
              <p className="text-sm text-neutral-400 italic">
                Download a summarization model in Settings to enable AI
                summaries.
              </p>
            ) : (
              <p className="text-sm text-neutral-400 italic">
                Click &ldquo;Summarize&rdquo; to generate an AI summary.
              </p>
            )}

            {summaryError && !summarizing && (
              <p className="text-xs text-red-500 dark:text-red-400 mt-2">
                {summaryError}
              </p>
            )}
          </TabsContent>

          <TabsContent
            value="transcript"
            className="flex-1 overflow-y-auto mt-4"
          >
            <div className="flex flex-col gap-3">
              <PersistedTranscript segments={detail.segments} />
            </div>
          </TabsContent>
        </Tabs>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Meetings list + detail container
// ---------------------------------------------------------------------------

function MeetingsView({
  meetingId,
  onTitleChange,
}: MeetingsViewProps) {
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [loading, setLoading] = useState(true);

  const {
    recording,
    meetingId: recordingMeetingId,
    segments: liveSegments,
    provisional,
    error,
  } = useRecording();

  const isLiveRecording =
    recording && detail != null && recordingMeetingId === detail.meeting.id;

  const openMeeting = useCallback(async (id: string) => {
    setLoading(true);
    try {
      const result = await invoke<MeetingDetail>("get_meeting", {
        meetingId: id,
      });
      setDetail(result);
    } catch (e) {
      console.error("Failed to load meeting:", e);
    }
    setLoading(false);
  }, []);

  // Load meeting when meetingId changes
  useEffect(() => {
    openMeeting(meetingId);
  }, [meetingId, openMeeting]);

  // When recording stops, reload meeting to get persisted segments from DB
  const wasLiveRef = useRef(false);
  useEffect(() => {
    if (wasLiveRef.current && !isLiveRecording && detail) {
      openMeeting(detail.meeting.id);
    }
    wasLiveRef.current = isLiveRecording;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isLiveRecording]);

  if (loading || !detail) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8">
        <p className="text-sm text-muted-foreground">Loading...</p>
      </div>
    );
  }

  return (
    <MeetingDetailView
      detail={detail}
      isLiveRecording={isLiveRecording}
      liveSegments={liveSegments}
      provisional={provisional}
      error={error}
      onTitleChange={onTitleChange}
    />
  );
}

export default MeetingsView;
