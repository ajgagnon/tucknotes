import { useEffect, useRef, useState } from "react";
import { useTauriEvent } from "@/hooks/use-tauri-event";

interface MinutesUpdatedPayload {
  meeting_id: string;
  body: string;
}

/**
 * Live meeting minutes during an active recording. The backend emits
 * `minutes:updated` with the full replacement body after each LLM pass
 * (replace-per-pass — the rolling rewrite would flicker if streamed).
 *
 * The subscription lives for as long as the meeting is open (not just while
 * recording): the finalize tail pass emits *after* the recording stops, and
 * the editor needs that final body too. `onMinutesBody` fires on every event
 * so the caller can keep its copy of the persisted document in sync — the
 * post-meeting editor hydrates from that copy.
 *
 * On the first event for a meeting that doesn't yet have a persisted
 * minutes document, triggers one meeting refresh so the new document (and
 * its tab) appears. When the recording ends the local body is cleared and
 * the persisted document body takes over.
 */
export function useLiveMinutes(
  meetingId: string,
  isLiveRecording: boolean,
  hasMinutesDoc: boolean,
  onRefreshMeeting?: () => void | Promise<void>,
  onMinutesBody?: (body: string) => void,
): { liveMinutesBody: string | null } {
  const [liveMinutesBody, setLiveMinutesBody] = useState<string | null>(null);
  const refreshedRef = useRef(false);
  const hasMinutesDocRef = useRef(hasMinutesDoc);
  hasMinutesDocRef.current = hasMinutesDoc;
  const onRefreshMeetingRef = useRef(onRefreshMeeting);
  onRefreshMeetingRef.current = onRefreshMeeting;
  const onMinutesBodyRef = useRef(onMinutesBody);
  onMinutesBodyRef.current = onMinutesBody;

  useEffect(() => {
    refreshedRef.current = false;
    setLiveMinutesBody(null);
  }, [meetingId]);

  useEffect(() => {
    if (!isLiveRecording) {
      setLiveMinutesBody(null);
    }
  }, [isLiveRecording]);

  useTauriEvent<MinutesUpdatedPayload>("minutes:updated", (payload) => {
    if (payload.meeting_id !== meetingId) return;
    setLiveMinutesBody(payload.body);
    onMinutesBodyRef.current?.(payload.body);
    if (!hasMinutesDocRef.current && !refreshedRef.current) {
      refreshedRef.current = true;
      void onRefreshMeetingRef.current?.();
    }
  });

  return { liveMinutesBody };
}
