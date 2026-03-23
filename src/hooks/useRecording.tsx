import {
  createContext,
  useContext,
  useState,
  useEffect,
  useRef,
  useCallback,
  useMemo,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { rmsToLevel, smoothLevel } from "../lib/audioLevel";
import { useTimer } from "./useTimer";

interface AudioChunkEvent {
  sample_count: number;
  rms: number;
  source: string;
  timestamp: number;
}

export interface TranscriptSegment {
  text: string;
  source: string;
  timestamp_ms: number;
  is_provisional: boolean;
}

export interface AppError {
  kind: string;
  message: string;
}

export function toAppError(e: unknown): AppError {
  if (typeof e === "object" && e !== null && "kind" in e && "message" in e) {
    return e as AppError;
  }
  return { kind: "Unknown", message: String(e) };
}

// ---------------------------------------------------------------------------
// Audio Level Context (high-frequency updates — only consumed by visualizer)
// ---------------------------------------------------------------------------

interface AudioLevelContextValue {
  systemLevel: number;
  micLevel: number;
}

const AudioLevelContext = createContext<AudioLevelContextValue | null>(null);

function AudioLevelProvider({ children }: { children: ReactNode }) {
  const { recording, paused } = useRecording();
  const levelsActive = recording && !paused;
  const [systemLevel, setSystemLevel] = useState(0);
  const [micLevel, setMicLevel] = useState(0);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  // Reset levels when recording stops
  useEffect(() => {
    if (!levelsActive) {
      setSystemLevel(0);
      setMicLevel(0);
    }
  }, [levelsActive]);

  // Time-based decay so levels drop consistently regardless of event frequency
  useEffect(() => {
    if (!levelsActive) return;
    const decay = setInterval(() => {
      setSystemLevel((l) => l * 0.85);
      setMicLevel((l) => l * 0.85);
    }, 150);
    return () => clearInterval(decay);
  }, [levelsActive]);

  // Audio-chunk event listener
  useEffect(() => {
    let mounted = true;
    (async () => {
      const unlisten = await listen<AudioChunkEvent>("audio-chunk", (event) => {
        if (!mounted) return;
        const { source, rms } = event.payload;
        const level = rmsToLevel(rms);
        if (source === "system") {
          setSystemLevel((prev) => smoothLevel(prev, level));
        } else {
          setMicLevel((prev) => smoothLevel(prev, level));
        }
      });
      if (mounted) unlistenRef.current = unlisten;
      else unlisten();
    })();
    return () => {
      mounted = false;
      unlistenRef.current?.();
    };
  }, []);

  const value = useMemo(
    () => ({ systemLevel, micLevel }),
    [systemLevel, micLevel],
  );

  return (
    <AudioLevelContext.Provider value={value}>
      {children}
    </AudioLevelContext.Provider>
  );
}

// ---------------------------------------------------------------------------
// Recording Context (low-frequency updates — recording state & transcript)
// ---------------------------------------------------------------------------

interface RecordingContextValue {
  /** True while audio capture is running (not paused). */
  recording: boolean;
  /** True when a meeting session is open but capture is paused (e.g. transcript sheet). */
  paused: boolean;
  meetingId: string | null;
  /** While set, persisted transcript for this meeting may still be catching up after stop. */
  transcriptFinalizingMeetingId: string | null;
  elapsed: number;
  error: AppError | null;
  segments: TranscriptSegment[];
  provisional: Record<string, TranscriptSegment>;
  startRecording: (resumeMeetingId?: string | null) => Promise<string>;
  stopRecording: () => Promise<void>;
  pauseRecording: () => Promise<void>;
  resumeRecording: () => Promise<void>;
  /** Replace live transcript (e.g. hydrate from a resumed meeting's saved segments). */
  seedLiveTranscript: (segments: TranscriptSegment[]) => void;
}

const RecordingContext = createContext<RecordingContextValue | null>(null);

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

function RecordingProviderInner({ children }: { children: ReactNode }) {
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

  // Sync recording state from backend on mount (handles window opened mid-recording)
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

  // Listen for recording-state-changed events (cross-window + own window sync)
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

  // Transcript event listener
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

// ---------------------------------------------------------------------------
// Exported provider (composes both contexts) and hooks
// ---------------------------------------------------------------------------

export function RecordingProvider({ children }: { children: ReactNode }) {
  return (
    <RecordingProviderInner>
      <AudioLevelProvider>{children}</AudioLevelProvider>
    </RecordingProviderInner>
  );
}

export function useRecording(): RecordingContextValue {
  const ctx = useContext(RecordingContext);
  if (!ctx)
    throw new Error("useRecording must be used within RecordingProvider");
  return ctx;
}

export function useAudioLevels(): AudioLevelContextValue {
  const ctx = useContext(AudioLevelContext);
  if (!ctx)
    throw new Error("useAudioLevels must be used within RecordingProvider");
  return ctx;
}
