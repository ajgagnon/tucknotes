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
import {
  FileText,
  ArrowLeft,
  Trash2,
  MoreVertical,
  Sparkles,
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { formatTime } from "@/lib/formatTime";
import {
  useRecording,
  type TranscriptSegment,
  type AppError,
} from "@/hooks/useRecording";
import { AnimatedShinyText } from "@/components/ui/animated-shiny-text";
import { Button } from "./ui/button";

interface MeetingRow {
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

interface MeetingDetail {
  meeting: MeetingRow;
  segments: SegmentRow[];
}

interface MeetingsViewProps {
  activeMeetingId?: string | null;
  onClearActiveMeeting?: () => void;
}

function formatDate(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
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
        {source === "system" ? "Speaker" : "You"}
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
  onBack: () => void;
  onDelete: () => void;
}

function MeetingDetailView({
  detail,
  isLiveRecording,
  liveSegments,
  provisional,
  error,
  onBack,
  onDelete,
}: MeetingDetailProps) {
  const transcriptEndRef = useRef<HTMLDivElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  // Title state
  const [currentTitle, setCurrentTitle] = useState(detail.meeting.title);
  const [editingTitle, setEditingTitle] = useState(false);
  const [generatingTitle, setGeneratingTitle] = useState(false);
  const titleInputRef = useRef<HTMLInputElement>(null);

  // Summarization state
  const [summarizing, setSummarizing] = useState(false);
  const [streamedSummary, setStreamedSummary] = useState("");
  const [thinkingText, setThinkingText] = useState("");
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [llmModelReady, setLlmModelReady] = useState<boolean | null>(null);
  const [currentSummary, setCurrentSummary] = useState<string | null>(
    detail.meeting.summary,
  );

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

  // Listen for AI-generated title (fires after summary, from background task)
  useEffect(() => {
    const unlisten = listen<string>("summary:title", (event) => {
      if (event.payload) {
        setCurrentTitle(event.payload);
      }
      setGeneratingTitle(false);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  async function saveTitle(newTitle: string) {
    const trimmed = newTitle.trim();
    setEditingTitle(false);
    if (trimmed && trimmed !== (currentTitle ?? "")) {
      setCurrentTitle(trimmed);
      try {
        await invoke("update_meeting_title", {
          meetingId: detail.meeting.id,
          title: trimmed,
        });
      } catch (e) {
        console.error("Failed to update title:", e);
      }
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

    setSummarizing(true);
    setGeneratingTitle(true);
    setStreamedSummary("");
    setThinkingText("");
    setSummaryError(null);

    const unlistenToken = await listen<string>("summary:token", (event) => {
      setStreamedSummary((prev) => prev + event.payload);
    });
    const unlistenThinking = await listen<string>(
      "summary:thinking",
      (event) => {
        setThinkingText((prev) => prev + event.payload);
      },
    );
    try {
      const summary = await invoke<string>("summarize_meeting", {
        meetingId: detail.meeting.id,
      });
      setCurrentSummary(summary);
    } catch (err) {
      const e = err as { message?: string };
      setSummaryError(e.message ?? "Summarization failed.");
    } finally {
      unlistenToken();
      unlistenThinking();
      setSummarizing(false);
      setStreamedSummary("");
      setThinkingText("");
    }
  }

  // Auto-scroll for live transcript
  useEffect(() => {
    if (isLiveRecording) {
      transcriptEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [liveSegments, provisional, isLiveRecording]);

  // Close menu on outside click
  useEffect(() => {
    if (!menuOpen) return;
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [menuOpen]);

  return (
    <div className="flex flex-col h-full p-6">
      <div className="flex items-center justify-between w-full mb-6">
        <div className="flex items-center gap-3">
          <button
            onClick={onBack}
            className="p-1.5 rounded-md hover:bg-black/5 dark:hover:bg-white/5 transition-colors cursor-pointer"
          >
            <ArrowLeft className="w-4 h-4" />
          </button>
          <div className="min-w-0">
            {editingTitle ? (
              <input
                ref={titleInputRef}
                className="text-lg font-semibold bg-transparent border-b border-primary outline-none w-full"
                defaultValue={currentTitle || ""}
                placeholder="Untitled"
                autoFocus
                onBlur={(e) => saveTitle(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.currentTarget.blur();
                  } else if (e.key === "Escape") {
                    setEditingTitle(false);
                  }
                }}
              />
            ) : (
              <h2
                className="text-lg font-semibold truncate cursor-text"
                onClick={() => {
                  if (!generatingTitle) setEditingTitle(true);
                }}
                title="Click to rename"
              >
                {generatingTitle ? (
                  <span className="flex items-center gap-2">
                    <span className="text-muted-foreground italic">Generating title…</span>
                    <span className="inline-block w-1.5 h-1.5 rounded-full bg-muted-foreground animate-pulse" />
                  </span>
                ) : (
                  currentTitle || "Untitled"
                )}
              </h2>
            )}
            <p className="text-xs text-neutral-400">
              {formatDate(detail.meeting.created_at)}
              {detail.meeting.duration_ms != null &&
                ` · ${formatTime(Math.floor(detail.meeting.duration_ms / 1000))}`}
            </p>
          </div>
        </div>
        <div className="relative" ref={menuRef}>
          <button
            onClick={() => setMenuOpen((prev) => !prev)}
            className="p-1.5 rounded-md text-neutral-400 hover:bg-black/5 dark:hover:bg-white/5 transition-colors cursor-pointer"
          >
            <MoreVertical className="w-4 h-4" />
          </button>
          {menuOpen && (
            <div className="absolute right-0 top-full mt-1 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg shadow-lg py-1 z-10 min-w-[140px]">
              <button
                onClick={() => {
                  setMenuOpen(false);
                  onDelete();
                }}
                className="flex items-center gap-2 w-full px-3 py-1.5 text-sm text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-500/10 transition-colors cursor-pointer"
              >
                <Trash2 className="w-3.5 h-3.5" />
                Delete
              </button>
            </div>
          )}
        </div>
      </div>

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
        <div className="flex-1 overflow-y-auto flex flex-col gap-3">
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
  activeMeetingId,
  onClearActiveMeeting,
}: MeetingsViewProps) {
  const [meetings, setMeetings] = useState<MeetingRow[]>([]);
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

  const loadMeetings = useCallback(async () => {
    setLoading(true);
    try {
      const result = await invoke<MeetingRow[]>("list_meetings");
      setMeetings(result);
    } catch (e) {
      console.error("Failed to load meetings:", e);
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    loadMeetings();
  }, [loadMeetings]);

  const openMeeting = useCallback(async (meetingId: string) => {
    try {
      const result = await invoke<MeetingDetail>("get_meeting", {
        meetingId,
      });
      setDetail(result);
    } catch (e) {
      console.error("Failed to load meeting:", e);
    }
  }, []);

  // When activeMeetingId is set (recording just started), open that meeting
  useEffect(() => {
    if (activeMeetingId) {
      openMeeting(activeMeetingId);
      onClearActiveMeeting?.();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeMeetingId]);

  // When recording stops, reload meeting to get persisted segments from DB
  const wasLiveRef = useRef(false);
  useEffect(() => {
    if (wasLiveRef.current && !isLiveRecording && detail) {
      openMeeting(detail.meeting.id);
    }
    wasLiveRef.current = isLiveRecording;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isLiveRecording]);

  const deleteMeeting = useCallback(
    async (meetingId: string) => {
      const confirmed = await ask("This action cannot be undone.", {
        title: "Delete meeting?",
        kind: "warning",
      });
      if (!confirmed) return;
      try {
        await invoke("delete_meeting", { meetingId });
        setMeetings((prev) => prev.filter((m) => m.id !== meetingId));
        if (detail?.meeting.id === meetingId) {
          setDetail(null);
        }
      } catch (e) {
        console.error("Failed to delete meeting:", e);
      }
    },
    [detail],
  );

  // Detail view
  if (detail) {
    return (
      <MeetingDetailView
        detail={detail}
        isLiveRecording={isLiveRecording}
        liveSegments={liveSegments}
        provisional={provisional}
        error={error}
        onBack={() => {
          setDetail(null);
          loadMeetings();
        }}
        onDelete={() => deleteMeeting(detail.meeting.id)}
      />
    );
  }

  if (loading) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8">
        <p className="text-sm text-muted-foreground">Loading...</p>
      </div>
    );
  }

  if (meetings.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <FileText className="w-12 h-12 text-muted-foreground mb-4" />
        <p className="text-sm text-muted-foreground">
          No meetings yet. Start a recording to create your first meeting.
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full p-5">
      <div className="flex flex-col gap-2">
        {meetings.map((meeting) => (
          <div
            key={meeting.id}
            className="flex items-center justify-between p-3 rounded-lg bg-black/4 dark:bg-white/4 hover:bg-black/6 dark:hover:bg-white/6 transition-colors cursor-pointer"
            onClick={() => openMeeting(meeting.id)}
          >
            <div className="flex flex-col gap-0.5 min-w-0">
              <span className="text-sm font-medium truncate">
                {meeting.title || "Untitled"}
              </span>
              <span className="text-xs text-neutral-400">
                {formatDate(meeting.created_at)}
                {meeting.duration_ms != null &&
                  ` · ${formatTime(Math.floor(meeting.duration_ms / 1000))}`}
              </span>
            </div>
            <button
              onClick={(e) => {
                e.stopPropagation();
                deleteMeeting(meeting.id);
              }}
              className="p-1.5 rounded-md text-neutral-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-500/10 transition-colors shrink-0 cursor-pointer"
              title="Delete meeting"
            >
              <Trash2 className="w-4 h-4" />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

export default MeetingsView;
