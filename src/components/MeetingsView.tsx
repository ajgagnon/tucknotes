import {
  useState,
  useEffect,
  useRef,
  useCallback,
  type RefObject,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { FileText, ArrowLeft, Trash2, MoreVertical } from "lucide-react";
import { formatTime } from "@/lib/formatTime";
import {
  useRecording,
  type TranscriptSegment,
  type AppError,
} from "@/hooks/useRecording";

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
          <div>
            <h2 className="text-lg font-semibold">
              {detail.meeting.title || "Untitled"}
            </h2>
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

      {/* Summary section — hidden during live recording */}
      {!isLiveRecording && (
        <div className="mb-6 rounded-lg bg-black/4 dark:bg-white/4 p-4">
          <h3 className="text-xs font-semibold uppercase tracking-wider text-neutral-400 mb-2">
            Summary
          </h3>
          {detail.meeting.summary ? (
            <p className="text-sm text-neutral-700 dark:text-neutral-300 leading-relaxed">
              {detail.meeting.summary}
            </p>
          ) : (
            <p className="text-sm text-neutral-400 italic">
              Summary will be available here.
            </p>
          )}
        </div>
      )}

      {/* Transcript section */}
      <h3 className="text-xs font-semibold uppercase tracking-wider text-neutral-400 mb-3">
        Transcript
      </h3>
      <div className="flex-1 overflow-y-auto flex flex-col gap-3">
        {isLiveRecording ? (
          <LiveTranscript
            segments={liveSegments}
            provisional={provisional}
            scrollRef={transcriptEndRef}
          />
        ) : (
          <PersistedTranscript segments={detail.segments} />
        )}
      </div>
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
    <div className="flex flex-col h-full p-6">
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
