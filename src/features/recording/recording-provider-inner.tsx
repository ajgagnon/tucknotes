import {
  useState,
  useEffect,
  useRef,
  useCallback,
  useMemo,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { RecordingContext } from "./recording-context";
import type { AppError, TranscriptSegment } from "./types";
import { toAppError } from "./types";
import { useTimer } from "./use-timer";

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
  const [error, setError] = useState<AppError | null>(null);
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [provisional, setProvisional] = useState<
    Record<string, TranscriptSegment>
  >({});

  const sessionActiveRef = useRef(false);

  useEffect(() => {
    sessionActiveRef.current = recording || paused;
  }, [recording, paused]);

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

  useEffect(() => {
    let mounted = true;
    let unlisten: UnlistenFn | null = null;
    (async () => {
      const fn_ = await listen<RecordingStateEvent>(
        "recording-state-changed",
        (event) => {
          if (!mounted) return;
          const {
            recording: isCapturing,
            paused: isPaused,
            meeting_id,
            elapsed_secs,
            reset_live_transcript: resetTranscript,
          } = event.payload;

          const wasSession = sessionActiveRef.current;
          const nowSession = isCapturing || isPaused;

          setRecording(isCapturing);
          setPaused(isPaused);

          if (nowSession) {
            if (meeting_id != null) setMeetingId(meeting_id);
            startTimer(elapsed_secs);
            if (
              !wasSession &&
              (resetTranscript === undefined || resetTranscript === true)
            ) {
              setSegments([]);
              setProvisional({});
            }
          } else {
            setMeetingId(null);
            clearTimer();
          }

          sessionActiveRef.current = nowSession;
        },
      );
      if (mounted) unlisten = fn_;
      else fn_();
    })();
    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [clearTimer, startTimer]);

  useEffect(() => {
    let mounted = true;
    let unlisten: UnlistenFn | null = null;
    (async () => {
      const fn_ = await listen<RecordingFinalizedEvent>(
        "recording-finalized",
        (event) => {
          if (!mounted) return;
          const id = event.payload.meeting_id;
          setTranscriptFinalizingMeetingId((cur) =>
            cur === id ? null : cur,
          );
        },
      );
      if (mounted) unlisten = fn_;
      else fn_();
    })();
    return () => {
      mounted = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let mounted = true;
    let unlisten: UnlistenFn | null = null;
    (async () => {
      const fn_ = await listen<TranscriptSegment>(
        "transcript-segment",
        (event) => {
          if (!mounted) return;
          const seg = event.payload;
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
        },
      );
      if (mounted) unlisten = fn_;
      else fn_();
    })();
    return () => {
      mounted = false;
      unlisten?.();
    };
  }, []);

  const startRecording = useCallback(
    async (resumeMeetingId?: string | null): Promise<string> => {
      setError(null);
      try {
        const args =
          resumeMeetingId != null && resumeMeetingId !== ""
            ? { resumeMeetingId }
            : {};
        return await invoke<string>("start_recording", args);
      } catch (e: unknown) {
        setError(toAppError(e));
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
      setError(toAppError(e));
    }
  }, []);

  const pauseRecording = useCallback(async () => {
    setError(null);
    try {
      await invoke("pause_recording");
    } catch (e: unknown) {
      setError(toAppError(e));
    }
  }, []);

  const resumeRecording = useCallback(async () => {
    setError(null);
    try {
      await invoke("resume_recording");
    } catch (e: unknown) {
      setError(toAppError(e));
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
      error,
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
      error,
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
