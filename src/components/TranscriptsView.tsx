import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileText, ArrowLeft, Trash2 } from "lucide-react";
import { formatTime } from "../lib/formatTime";

interface SessionRow {
  id: string;
  title: string | null;
  created_at: number;
  ended_at: number | null;
  duration_ms: number | null;
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

interface SessionDetail {
  session: SessionRow;
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

function TranscriptsView() {
  const [sessions, setSessions] = useState<SessionRow[]>([]);
  const [detail, setDetail] = useState<SessionDetail | null>(null);
  const [loading, setLoading] = useState(true);

  const loadSessions = async () => {
    setLoading(true);
    try {
      const result = await invoke<SessionRow[]>("list_sessions");
      setSessions(result);
    } catch (e) {
      console.error("Failed to load sessions:", e);
    }
    setLoading(false);
  };

  useEffect(() => {
    loadSessions();
  }, []);

  const openSession = async (sessionId: string) => {
    try {
      const result = await invoke<SessionDetail>("get_session", {
        sessionId,
      });
      setDetail(result);
    } catch (e) {
      console.error("Failed to load session:", e);
    }
  };

  const deleteSession = async (sessionId: string) => {
    try {
      await invoke("delete_session", { sessionId });
      setSessions((prev) => prev.filter((s) => s.id !== sessionId));
      if (detail?.session.id === sessionId) {
        setDetail(null);
      }
    } catch (e) {
      console.error("Failed to delete session:", e);
    }
  };

  if (detail) {
    return (
      <div className="flex flex-col h-full p-6">
        <div className="flex items-center gap-3 mb-6">
          <button
            onClick={() => setDetail(null)}
            className="p-1.5 rounded-md hover:bg-black/5 dark:hover:bg-white/5 transition-colors cursor-pointer"
          >
            <ArrowLeft className="w-4 h-4" />
          </button>
          <div>
            <h2 className="text-lg font-semibold">
              {detail.session.title || "Untitled"}
            </h2>
            <p className="text-xs text-neutral-400">
              {formatDate(detail.session.created_at)}
              {detail.session.duration_ms != null &&
                ` · ${formatTime(Math.floor(detail.session.duration_ms / 1000))}`}
            </p>
          </div>
        </div>

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

  if (sessions.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <FileText className="w-12 h-12 text-muted-foreground mb-4" />
        <p className="text-sm text-muted-foreground">
          Your saved transcripts will appear here.
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full p-6">
      <div className="flex flex-col gap-2">
        {sessions.map((session) => (
          <div
            key={session.id}
            className="flex items-center justify-between p-3 rounded-lg bg-black/4 dark:bg-white/4 hover:bg-black/6 dark:hover:bg-white/6 transition-colors cursor-pointer"
            onClick={() => openSession(session.id)}
          >
            <div className="flex flex-col gap-0.5 min-w-0">
              <span className="text-sm font-medium truncate">
                {session.title || "Untitled"}
              </span>
              <span className="text-xs text-neutral-400">
                {formatDate(session.created_at)}
                {session.duration_ms != null &&
                  ` · ${formatTime(Math.floor(session.duration_ms / 1000))}`}
              </span>
            </div>
            <button
              onClick={(e) => {
                e.stopPropagation();
                deleteSession(session.id);
              }}
              className="p-1.5 rounded-md text-neutral-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-500/10 transition-colors shrink-0 cursor-pointer"
              title="Delete session"
            >
              <Trash2 className="w-4 h-4" />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

export default TranscriptsView;
