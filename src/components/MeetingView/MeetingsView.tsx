import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useRecording } from "@/hooks/useRecording";
import { type MeetingDetail, type MeetingTitleInfo } from "./types";
import { MeetingDetailView } from "./MeetingDetailView";

interface MeetingsViewProps {
  meetingId: string;
  onTitleChange?: (info: MeetingTitleInfo) => void;
}

export default function MeetingsView({
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
    />
  );
}
