import {
  useState,
  useEffect,
  useRef,
  useCallback,
  useMemo,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "@/hooks/use-tauri-event";
import { RecordingContext } from "./recording-context";
import type { AppError, TranscriptSegment } from "./types";
import { toAppError } from "./types";
import { useTimer } from "./use-timer";
import { useAutoStop } from "./use-auto-stop";
import { toastError } from "@/lib/toast";

/** Surface a recording failure as a toast, with a settings action for the
 *  permission case. */
function notifyRecordingError(e: unknown) {
  const err = toAppError(e);
  if (err.kind === "PermissionDenied") {
    toastError("Permission needed to capture audio", {
      description: "Enable Screen Recording in macOS settings to get started.",
      action: {
        label: "Open System Settings",
        onClick: () => {
          void invoke("open_screen_recording_settings");
        },
      },
    });
    return;
  }
  toastError(err.message);
}

interface RecordingStateEvent {
  recording: boolean;
  paused: boolean;
  meeting_id: string | null;
  elapsed_secs: number;
  /** When false, keep existing live segments (resumed meeting). */
  reset_live_transcript?: boolean;
}

interface RecordingFinalizedEvent {
  meeting_id: string;
}

export function RecordingProviderInner({ children }: { children: ReactNode }) {
  const [recording, setRecording] = useState(false);
  const [paused, setPaused] = useState(false);
  const [meetingId, setMeetingId] = useState<string | null>(null);
  const [transcriptFinalizingMeetingId, setTranscriptFinalizingMeetingId] =
    useState<string | null>(null);
  const { elapsed, startTimer, clearTimer } = useTimer();
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [provisional, setProvisional] = useState<
    Record<string, TranscriptSegment>
  >({});

  const sessionActiveRef = useRef(false);

  useEffect(() => {
    sessionActiveRef.current = recording || paused;
  }, [recording, paused]);

  const { armForNewSession, disarm, hideOverlay } =
    useAutoStop(sessionActiveRef);

  useEffect(() => {
    (async () => {
      try {
        const state = await invoke<RecordingStateEvent>("get_recording_state");
        if (state.recording || state.paused) {
          setRecording(state.recording);
          setPaused(state.paused);
          setMeetingId(state.meeting_id);
          startTimer(state.elapsed_secs);
        }
      } catch (e) {
        console.error("Failed to get recording state:", e);
      }
    })();
    return clearTimer;
  }, [clearTimer, startTimer]);

  useTauriEvent<RecordingStateEvent>("recording-state-changed", (payload) => {
    const {
      recording: isCapturing,
      paused: isPaused,
      meeting_id,
      elapsed_secs,
      reset_live_transcript: resetTranscript,
    } = payload;

    const wasSession = sessionActiveRef.current;
    const nowSession = isCapturing || isPaused;

    setRecording(isCapturing);
    setPaused(isPaused);

    if (nowSession) {
      if (meeting_id != null) setMeetingId(meeting_id);
      startTimer(elapsed_secs);
      if (!wasSession) {
        armForNewSession();
        if (resetTranscript === undefined || resetTranscript === true) {
          setSegments([]);
          setProvisional({});
        }
      } else if (isPaused) {
        // Pause is an explicit "I'm not done" — dismiss any open prompt.
        hideOverlay();
      }
    } else {
      setMeetingId(null);
      clearTimer();
      disarm();
    }

    sessionActiveRef.current = nowSession;
  });

  // Capture startup runs in the background after start_recording returns;
  // failures there arrive as events instead of a rejected invoke.
  useTauriEvent<AppError>("recording-error", (error) => {
    notifyRecordingError(error);
  });

  useTauriEvent<RecordingFinalizedEvent>("recording-finalized", (payload) => {
    const id = payload.meeting_id;
    setTranscriptFinalizingMeetingId((cur) => (cur === id ? null : cur));
  });

  useTauriEvent<TranscriptSegment>("transcript-segment", (seg) => {
    if (seg.is_provisional) {
      setProvisional((prev) => ({ ...prev, [seg.source]: seg }));
    } else {
      setSegments((prev) => [...prev, seg]);
      setProvisional((prev) => {
        const next = { ...prev };
        delete next[seg.source];
        return next;
      });
    }
  });

  const startRecording = useCallback(
    async (resumeMeetingId?: string | null): Promise<string> => {
      try {
        const args =
          resumeMeetingId != null && resumeMeetingId !== ""
            ? { resumeMeetingId }
            : {};
        return await invoke<string>("start_recording", args);
      } catch (e: unknown) {
        notifyRecordingError(e);
        throw e;
      }
    },
    [],
  );

  const stopRecording = useCallback(async () => {
    try {
      const pending = await invoke<string | null>("stop_recording");
      if (pending) {
        setTranscriptFinalizingMeetingId(pending);
      }
    } catch (e: unknown) {
      notifyRecordingError(e);
    }
  }, []);

  const pauseRecording = useCallback(async () => {
    try {
      await invoke("pause_recording");
    } catch (e: unknown) {
      notifyRecordingError(e);
    }
  }, []);

  const resumeRecording = useCallback(async () => {
    try {
      await invoke("resume_recording");
    } catch (e: unknown) {
      notifyRecordingError(e);
    }
  }, []);

  const seedLiveTranscript = useCallback((next: TranscriptSegment[]) => {
    setSegments(next);
    setProvisional({});
  }, []);

  const value = useMemo(
    () => ({
      recording,
      paused,
      meetingId,
      transcriptFinalizingMeetingId,
      elapsed,
      segments,
      provisional,
      startRecording,
      stopRecording,
      pauseRecording,
      resumeRecording,
      seedLiveTranscript,
    }),
    [
      recording,
      paused,
      meetingId,
      transcriptFinalizingMeetingId,
      elapsed,
      segments,
      provisional,
      startRecording,
      stopRecording,
      pauseRecording,
      resumeRecording,
      seedLiveTranscript,
    ],
  );

  return (
    <RecordingContext.Provider value={value}>
      {children}
    </RecordingContext.Provider>
  );
}
