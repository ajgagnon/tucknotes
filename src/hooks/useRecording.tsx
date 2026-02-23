import {
  createContext,
  useContext,
  useState,
  useEffect,
  useRef,
  useCallback,
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

interface RecordingContextValue {
  recording: boolean;
  elapsed: number;
  error: AppError | null;
  systemLevel: number;
  micLevel: number;
  segments: TranscriptSegment[];
  provisional: Record<string, TranscriptSegment>;
  startRecording: () => Promise<void>;
  stopRecording: () => Promise<void>;
  toggleRecording: () => void;
}

const RecordingContext = createContext<RecordingContextValue | null>(null);

export function RecordingProvider({ children }: { children: ReactNode }) {
  const [recording, setRecording] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [error, setError] = useState<AppError | null>(null);
  const [systemLevel, setSystemLevel] = useState(0);
  const [micLevel, setMicLevel] = useState(0);
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [provisional, setProvisional] = useState<
    Record<string, TranscriptSegment>
  >({});

  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const unlistenRef = useRef<UnlistenFn | null>(null);
  const transcriptUnlistenRef = useRef<UnlistenFn | null>(null);

  const clearTimer = useCallback(() => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const startRecording = useCallback(async () => {
    setError(null);
    try {
      await invoke("start_recording");
      setRecording(true);
      setElapsed(0);
      setSegments([]);
      setProvisional({});

      timerRef.current = setInterval(() => {
        setElapsed((prev) => prev + 1);
      }, 1000);
    } catch (e: unknown) {
      setError(e as AppError);
      throw e;
    }
  }, []);

  const stopRecording = useCallback(async () => {
    try {
      await invoke("stop_recording");
    } catch (e: unknown) {
      setError(e as AppError);
    }
    setRecording(false);
    clearTimer();
    setSystemLevel(0);
    setMicLevel(0);
  }, [clearTimer]);

  const toggleRecording = useCallback(() => {
    if (recording) {
      stopRecording();
    } else {
      startRecording();
    }
  }, [recording, startRecording, stopRecording]);

  // Set up Tauri event listeners
  useEffect(() => {
    let mounted = true;

    listen<AudioChunkEvent>("audio-chunk", (event) => {
      if (!mounted) return;
      const { source, rms } = event.payload;
      const level = rmsToLevel(rms);

      if (source === "system") {
        setSystemLevel((prev) => smoothLevel(prev, level));
      } else {
        setMicLevel((prev) => smoothLevel(prev, level));
      }
    }).then((unlisten) => {
      unlistenRef.current = unlisten;
    });

    listen<TranscriptSegment>("transcript-segment", (event) => {
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
    }).then((unlisten) => {
      transcriptUnlistenRef.current = unlisten;
    });

    return () => {
      mounted = false;
      unlistenRef.current?.();
      transcriptUnlistenRef.current?.();
    };
  }, []);

  // Clean up timer on unmount
  useEffect(() => {
    return clearTimer;
  }, [clearTimer]);

  // Audio level decay while recording
  useEffect(() => {
    if (!recording) return;
    const decay = setInterval(() => {
      setSystemLevel((l) => l * 0.85);
      setMicLevel((l) => l * 0.85);
    }, 150);
    return () => clearInterval(decay);
  }, [recording]);

  return (
    <RecordingContext.Provider
      value={{
        recording,
        elapsed,
        error,
        systemLevel,
        micLevel,
        segments,
        provisional,
        startRecording,
        stopRecording,
        toggleRecording,
      }}
    >
      {children}
    </RecordingContext.Provider>
  );
}

export function useRecording(): RecordingContextValue {
  const ctx = useContext(RecordingContext);
  if (!ctx)
    throw new Error("useRecording must be used within RecordingProvider");
  return ctx;
}
