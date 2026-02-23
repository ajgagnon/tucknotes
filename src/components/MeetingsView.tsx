import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileText, ArrowLeft, Trash2 } from "lucide-react";
import { formatTime } from "../lib/formatTime";

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
  session_id: string;
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

function formatDate(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function MeetingsView() {
  const [meetings, setMeetings] = useState<MeetingRow[]>([]);
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [loading, setLoading] = useState(true);

  const loadMeetings = async () => {
    setLoading(true);
    try {
      const result = await invoke<MeetingRow[]>("list_meetings");
      setMeetings(result);
    } catch (e) {
      console.error("Failed to load meetings:", e);
    }
    setLoading(false);
  };

  useEffect(() => {
    loadMeetings();
  }, []);

  const openMeeting = async (meetingId: string) => {
    try {
      const result = await invoke<MeetingDetail>("get_meeting", {
        meetingId,
      });
      setDetail(result);
    } catch (e) {
      console.error("Failed to load meeting:", e);
    }
  };

  const deleteMeeting = async (meetingId: string) => {
    try {
      await invoke("delete_meeting", { meetingId });
      setMeetings((prev) => prev.filter((m) => m.id !== meetingId));
      if (detail?.meeting.id === meetingId) {
        setDetail(null);
      }
    } catch (e) {
      console.error("Failed to delete meeting:", e);
    }
  };

  if (detail) {
    return (
      <div className="flex flex-col h-full p-6">
        <div className="flex items-center gap-3 mb-6">
          <button
            onClick={() => {
              setDetail(null);
              loadMeetings();
            }}
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

        {/* Summary section */}
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

        {/* Transcript section */}
        <h3 className="text-xs font-semibold uppercase tracking-wider text-neutral-400 mb-3">
          Transcript
        </h3>
        <div className="flex-1 overflow-y-auto flex flex-col gap-3">
          {detail.segments.length === 0 ? (
            <p className="text-sm text-neutral-400 text-center mt-8">
              No transcript segments recorded.
            </p>
          ) : (
            detail.segments.map((seg) => (
              <div key={seg.id} className="flex flex-col gap-0.5">
                <span
                  className={`text-[0.65rem] font-semibold uppercase tracking-wider ${
                    seg.source === "system" ? "text-primary" : "text-success"
                  }`}
                >
                  {seg.source === "system" ? "System" : "Mic"}
                </span>
                <p className="text-sm text-neutral-700 dark:text-neutral-300 m-0 leading-snug">
                  {seg.text}
                </p>
              </div>
            ))
          )}
        </div>
      </div>
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
