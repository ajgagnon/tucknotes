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
  elapsed: number;
  error: AppError | null;
  segments: TranscriptSegment[];
  provisional: Record<string, TranscriptSegment>;
  startRecording: () => Promise<string>;
  stopRecording: () => Promise<void>;
}

const RecordingContext = createContext<RecordingContextValue | null>(null);

function RecordingProviderInner({ children }: { children: ReactNode }) {
  const [recording, setRecording] = useState(false);
  const [meetingId, setMeetingId] = useState<string | null>(null);
  const [elapsed, setElapsed] = useState(0);
  const [error, setError] = useState<AppError | null>(null);
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [provisional, setProvisional] = useState<
    Record<string, TranscriptSegment>
  >({});

  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const transcriptUnlistenRef = useRef<UnlistenFn | null>(null);

  const clearTimer = useCallback(() => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const startRecording = useCallback(async (): Promise<string> => {
    setError(null);
    try {
      const id = await invoke<string>("start_recording");
      setMeetingId(id);
      setRecording(true);
      setElapsed(0);
      setSegments([]);
      setProvisional({});

      timerRef.current = setInterval(() => {
        setElapsed((prev) => prev + 1);
      }, 1000);
      return id;
    } catch (e: unknown) {
      setError(toAppError(e));
      throw e;
    }
  }, []);

  const stopRecording = useCallback(async () => {
    try {
      await invoke("stop_recording");
      setRecording(false);
      clearTimer();
    } catch (e: unknown) {
      setError(toAppError(e));
    }
  }, [clearTimer]);

  // Transcript event listener
  useEffect(() => {
    let mounted = true;
    (async () => {
      const unlisten = await listen<TranscriptSegment>(
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
      if (mounted) transcriptUnlistenRef.current = unlisten;
      else unlisten();
    })();
    return () => {
      mounted = false;
      transcriptUnlistenRef.current?.();
    };
  }, []);

  // Clean up timer on unmount
  useEffect(() => {
    return clearTimer;
  }, [clearTimer]);

  const value = useMemo(
    () => ({
      recording,
      meetingId,
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
