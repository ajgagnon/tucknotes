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

interface MeetingDetectedEvent {
  phase: "Idle" | "Detecting" | "Active" | "Ending";
  app_name: string | null;
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
  const matchedMeetingRef = useRef(false);
  const matchedAppNameRef = useRef<string | null>(null);
  // Once the user clicks "Keep" on the auto-stop prompt, suppress further
  // prompts for the rest of this recording session — don't nag on repeated
  // false positives from the AX-based detector.
  const autoStopDismissedRef = useRef(false);

  useEffect(() => {
    sessionActiveRef.current = recording || paused;
  }, [recording, paused]);

  const hideAutoStopOverlay = useCallback(() => {
    void invoke("hide_auto_stop_overlay").catch(() => {});
  }, []);

  const resetAutoStopForNewSession = useCallback(() => {
    matchedMeetingRef.current = false;
    matchedAppNameRef.current = null;
    autoStopDismissedRef.current = false;
    hideAutoStopOverlay();
  }, [hideAutoStopOverlay]);

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
            if (!wasSession) {
              resetAutoStopForNewSession();
              // Recover state when recording starts after a meeting was already
              // detected — otherwise no `meeting-detected` event arrives during
              // this session and auto-stop would never arm.
              void invoke<string | null>("get_current_meeting_app")
                .then((appName) => {
                  if (!mounted) return;
                  if (appName) {
                    matchedMeetingRef.current = true;
                    matchedAppNameRef.current = appName;
                  }
                })
                .catch(() => {});
              if (resetTranscript === undefined || resetTranscript === true) {
                setSegments([]);
                setProvisional({});
              }
            } else if (isPaused) {
              // Pause is an explicit "I'm not done" — dismiss any open prompt.
              hideAutoStopOverlay();
            }
          } else {
            setMeetingId(null);
            clearTimer();
            resetAutoStopForNewSession();
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
  }, [clearTimer, hideAutoStopOverlay, resetAutoStopForNewSession, startTimer]);

  // Capture startup runs in the background after start_recording returns;
  // failures there arrive as events instead of a rejected invoke.
  useEffect(() => {
    let mounted = true;
    let unlisten: UnlistenFn | null = null;
    (async () => {
      const fn_ = await listen<AppError>("recording-error", (event) => {
        if (!mounted) return;
        notifyRecordingError(event.payload);
      });
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

  useEffect(() => {
    let mounted = true;
    let unlisten: UnlistenFn | null = null;
    (async () => {
      const fn_ = await listen<MeetingDetectedEvent>(
        "meeting-detected",
        (event) => {
          if (!mounted) return;
          if (!sessionActiveRef.current) return;
          matchedMeetingRef.current = true;
          matchedAppNameRef.current = event.payload.app_name;
          // Detector saw the call again — close any stale prompt (false positive
          // recovered, or user rejoined).
          hideAutoStopOverlay();
        },
      );
      if (mounted) unlisten = fn_;
      else fn_();
    })();
    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [hideAutoStopOverlay]);

  useEffect(() => {
    let mounted = true;
    let unlisten: UnlistenFn | null = null;
    (async () => {
      const fn_ = await listen("auto-stop-cancel-requested", () => {
        if (!mounted) return;
        autoStopDismissedRef.current = true;
      });
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

  useEffect(() => {
    let mounted = true;
    let unlisten: UnlistenFn | null = null;
    (async () => {
      const fn_ = await listen<MeetingDetectedEvent>("meeting-ended", () => {
        if (!mounted) return;
        if (!sessionActiveRef.current) return;
        if (!matchedMeetingRef.current) return;
        if (autoStopDismissedRef.current) return;

        void invoke("show_auto_stop_overlay", {
          appName: matchedAppNameRef.current,
        }).catch((e) => console.error("show_auto_stop_overlay:", e));
      });
      if (mounted) unlisten = fn_;
      else fn_();
    })();
    return () => {
      mounted = false;
      unlisten?.();
    };
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
