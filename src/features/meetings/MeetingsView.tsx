import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useRecording } from "@/features/recording";
import { type MeetingDetail, type MeetingTitleInfo } from "./types";
import { MeetingDetailView } from "./MeetingDetailView";

interface MeetingsViewProps {
  meetingId: string;
  onTitleChange?: (info: MeetingTitleInfo) => void;
  onRecordingStarted?: (meetingId: string) => void;
  onOpenSettings?: () => void;
}

export default function MeetingsView({
  meetingId,
  onTitleChange,
  onRecordingStarted,
  onOpenSettings,
}: MeetingsViewProps) {
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [loading, setLoading] = useState(true);

  const {
    recording,
    paused,
    meetingId: recordingMeetingId,
    segments: liveSegments,
    provisional,
    error,
  } = useRecording();

  const isLiveRecording =
    (recording || paused) &&
    detail != null &&
    recordingMeetingId === detail.meeting.id;

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

  const handleMeetingDocumentBodyUpdated = useCallback(
    (documentId: string, body: string) => {
      setDetail((prev) => {
        if (!prev) return prev;
        return {
          ...prev,
          documents: prev.documents.map((d) =>
            d.id === documentId ? { ...d, body } : d,
          ),
        };
      });
    },
    [],
  );

  useEffect(() => {
    openMeeting(meetingId);
  }, [meetingId, openMeeting]);

  const wasLiveRef = useRef(false);
  useEffect(() => {
    if (wasLiveRef.current && !isLiveRecording && detail) {
      openMeeting(detail.meeting.id);
    }
    wasLiveRef.current = isLiveRecording;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isLiveRecording]);

  useEffect(() => {
    let cancelled = false;
    const unlisten = listen<{ meeting_id: string }>(
      "recording-finalized",
      (event) => {
        if (cancelled || event.payload.meeting_id !== meetingId) return;
        openMeeting(meetingId);
      },
    );
    return () => {
      cancelled = true;
      unlisten.then((fn) => fn());
    };
  }, [meetingId, openMeeting]);

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
      onRecordingStarted={onRecordingStarted}
      onRefreshMeeting={() => openMeeting(detail.meeting.id)}
      onMeetingDocumentBodyUpdated={handleMeetingDocumentBodyUpdated}
      onOpenSettings={onOpenSettings}
    />
  );
}
