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
  const { recording } = useRecording();
  const [systemLevel, setSystemLevel] = useState(0);
  const [micLevel, setMicLevel] = useState(0);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  // Reset levels when recording stops
  useEffect(() => {
    if (!recording) {
      setSystemLevel(0);
      setMicLevel(0);
    }
  }, [recording]);

  // Time-based decay so levels drop consistently regardless of event frequency
  useEffect(() => {
    if (!recording) return;
    const decay = setInterval(() => {
      setSystemLevel((l) => l * 0.85);
      setMicLevel((l) => l * 0.85);
    }, 150);
    return () => clearInterval(decay);
  }, [recording]);

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
  recording: boolean;
  meetingId: string | null;
  /** While set, persisted transcript for this meeting may still be catching up after stop. */
  transcriptFinalizingMeetingId: string | null;
  elapsed: number;
  error: AppError | null;
  segments: TranscriptSegment[];
  provisional: Record<string, TranscriptSegment>;
  startRecording: () => Promise<string>;
  stopRecording: () => Promise<void>;
}

const RecordingContext = createContext<RecordingContextValue | null>(null);

interface RecordingStateEvent {
  recording: boolean;
  meeting_id: string | null;
  elapsed_secs: number;
}

interface RecordingFinalizedEvent {
  meeting_id: string;
}

function RecordingProviderInner({ children }: { children: ReactNode }) {
  const [recording, setRecording] = useState(false);
  const [meetingId, setMeetingId] = useState<string | null>(null);
  const [transcriptFinalizingMeetingId, setTranscriptFinalizingMeetingId] =
    useState<string | null>(null);
  const { elapsed, startTimer, clearTimer } = useTimer();
  const [error, setError] = useState<AppError | null>(null);
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [provisional, setProvisional] = useState<
    Record<string, TranscriptSegment>
  >({});

  const recordingRef = useRef(false);

  // Keep ref in sync with state so event handlers see latest value
  useEffect(() => {
    recordingRef.current = recording;
  }, [recording]);

  // Sync recording state from backend on mount (handles window opened mid-recording)
  useEffect(() => {
    (async () => {
      try {
        const state = await invoke<RecordingStateEvent>("get_recording_state");
        if (state.recording) {
          setRecording(true);
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
            recording: isRecording,
            meeting_id,
            elapsed_secs,
          } = event.payload;

          if (isRecording && !recordingRef.current) {
            // Transitioning to recording
            setRecording(true);
            setMeetingId(meeting_id);
            setSegments([]);
            setProvisional({});
            startTimer(elapsed_secs);
          } else if (!isRecording && recordingRef.current) {
            // Transitioning to not recording
            setRecording(false);
            clearTimer();
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

  const startRecording = useCallback(async (): Promise<string> => {
    setError(null);
    try {
      return await invoke<string>("start_recording");
    } catch (e: unknown) {
      setError(toAppError(e));
      throw e;
    }
  }, []);

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

  const value = useMemo(
    () => ({
      recording,
      meetingId,
      transcriptFinalizingMeetingId,
      elapsed,
      error,
      segments,
      provisional,
      startRecording,
      stopRecording,
    }),
    [
      recording,
      meetingId,
      transcriptFinalizingMeetingId,
      elapsed,
      error,
      segments,
      provisional,
      startRecording,
      stopRecording,
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
