import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { formatTime } from "../lib/formatTime";
import { rmsToLevel, smoothLevel } from "../lib/audioLevel";

interface AudioChunkEvent {
  sample_count: number;
  rms: number;
  source: string;
  timestamp: number;
}

interface TranscriptSegment {
  text: string;
  source: string;
  timestamp_ms: number;
  is_provisional: boolean;
}

function RecordingView() {
  const [recording, setRecording] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [systemChunks, setSystemChunks] = useState(0);
  const [micChunks, setMicChunks] = useState(0);
  const [systemLevel, setSystemLevel] = useState(0);
  const [micLevel, setMicLevel] = useState(0);
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [provisional, setProvisional] = useState<
    Record<string, TranscriptSegment>
  >({});

  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const unlistenRef = useRef<UnlistenFn | null>(null);
  const transcriptUnlistenRef = useRef<UnlistenFn | null>(null);
  const transcriptEndRef = useRef<HTMLDivElement | null>(null);

  const clearTimer = useCallback(() => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const startRecording = async () => {
    setError(null);
    try {
      await invoke("start_recording");
      setRecording(true);
      setElapsed(0);
      setSystemChunks(0);
      setMicChunks(0);
      setSegments([]);
      setProvisional({});

      timerRef.current = setInterval(() => {
        setElapsed((prev) => prev + 1);
      }, 1000);
    } catch (e) {
      setError(String(e));
    }
  };

  const stopRecording = async () => {
    try {
      await invoke("stop_recording");
    } catch (e) {
      setError(String(e));
    }
    setRecording(false);
    clearTimer();
    setSystemLevel(0);
    setMicLevel(0);
  };

  const handleToggle = () => {
    if (recording) {
      stopRecording();
    } else {
      startRecording();
    }
  };

  useEffect(() => {
    let mounted = true;

    listen<AudioChunkEvent>("audio-chunk", (event) => {
      if (!mounted) return;
      const { source, rms } = event.payload;
      const level = rmsToLevel(rms);

      if (source === "system") {
        setSystemChunks((c) => c + 1);
        setSystemLevel((prev) => smoothLevel(prev, level));
      } else {
        setMicChunks((c) => c + 1);
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

  useEffect(() => {
    return clearTimer;
  }, [clearTimer]);

  useEffect(() => {
    transcriptEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [segments, provisional]);

  useEffect(() => {
    if (!recording) return;
    const decay = setInterval(() => {
      setSystemLevel((l) => l * 0.85);
      setMicLevel((l) => l * 0.85);
    }, 150);
    return () => clearInterval(decay);
  }, [recording]);

  return (
    <div className="h-full flex flex-col items-center justify-center p-8 gap-8">
      {recording && (
        <p className="text-[2.5rem] font-light tabular-nums tracking-wide text-danger m-0">
          {formatTime(elapsed)}
        </p>
      )}

      <button
        className={`w-24 h-24 rounded-full border-4 flex items-center justify-center cursor-pointer transition-all duration-200 relative ${
          recording
            ? "border-danger/30 shadow-[0_0_0_6px_rgba(229,62,62,0.1),0_4px_20px_rgba(229,62,62,0.15)] dark:border-danger/40 dark:shadow-[0_0_0_6px_rgba(229,62,62,0.15),0_4px_20px_rgba(229,62,62,0.2)] animate-pulse-ring"
            : "border-black/8 bg-neutral-100 shadow-[0_2px_12px_rgba(0,0,0,0.08)] dark:bg-neutral-700 dark:border-white/8 dark:shadow-[0_2px_12px_rgba(0,0,0,0.3)]"
        } hover:scale-105 hover:shadow-[0_4px_20px_rgba(0,0,0,0.12)] dark:hover:shadow-[0_4px_20px_rgba(0,0,0,0.4)] active:scale-[0.97]`}
        onClick={handleToggle}
        title={recording ? "Stop recording" : "Start recording"}
      >
        <span
          className={`bg-danger transition-all duration-250 ${
            recording ? "w-7 h-7 rounded-md" : "w-10 h-10 rounded-full"
          }`}
        />
      </button>

      <div className="text-center">
        <h2
          className={`text-lg font-semibold mb-1 ${
            recording ? "text-danger" : "text-neutral-500 dark:text-neutral-400"
          }`}
        >
          {recording ? "Recording" : "Ready"}
        </h2>
        <p className="text-sm text-neutral-400 m-0">
          {recording
            ? "Capturing system audio & microphone"
            : "Tap to start recording your meeting"}
        </p>
      </div>

      {recording && (
        <>
          <div className="flex flex-col gap-2.5 w-full max-w-80">
            <div className="flex items-center gap-3">
              <span className="text-xs font-medium text-neutral-400 w-[70px] text-right shrink-0 uppercase tracking-wider">
                System
              </span>
              <div className="flex-1 h-1 bg-black/6 dark:bg-white/8 rounded-sm overflow-hidden">
                <div
                  className="h-full rounded-sm transition-[width] duration-100 ease-out min-w-0.5 bg-primary"
                  style={{ width: `${systemLevel * 100}%` }}
                />
              </div>
            </div>
            <div className="flex items-center gap-3">
              <span className="text-xs font-medium text-neutral-400 w-[70px] text-right shrink-0 uppercase tracking-wider">
                Mic
              </span>
              <div className="flex-1 h-1 bg-black/6 dark:bg-white/8 rounded-sm overflow-hidden">
                <div
                  className="h-full rounded-sm transition-[width] duration-100 ease-out min-w-0.5 bg-success"
                  style={{ width: `${micLevel * 100}%` }}
                />
              </div>
            </div>
          </div>

          <div className="flex gap-6 justify-center">
            <div className="text-center">
              <span className="text-xl font-semibold block tabular-nums text-primary">
                {systemChunks}
              </span>
              <span className="text-[0.7rem] text-neutral-400 uppercase tracking-wider">
                System chunks
              </span>
            </div>
            <div className="text-center">
              <span className="text-xl font-semibold block tabular-nums text-success">
                {micChunks}
              </span>
              <span className="text-[0.7rem] text-neutral-400 uppercase tracking-wider">
                Mic chunks
              </span>
            </div>
          </div>

          <div className="w-full max-w-80 max-h-64 overflow-y-auto rounded-lg bg-black/4 dark:bg-white/4 p-3 flex flex-col gap-2">
            {segments.length === 0 &&
            Object.keys(provisional).length === 0 ? (
              <p className="text-xs text-neutral-400 text-center m-0">
                Transcript will appear here...
              </p>
            ) : (
              <>
                {segments.map((seg, i) => (
                  <div key={i} className="flex flex-col gap-0.5">
                    <span
                      className={`text-[0.65rem] font-semibold uppercase tracking-wider ${
                        seg.source === "system"
                          ? "text-primary"
                          : "text-success"
                      }`}
                    >
                      {seg.source === "system" ? "System" : "Mic"}
                    </span>
                    <p className="text-sm text-neutral-700 dark:text-neutral-300 m-0 leading-snug">
                      {seg.text}
                    </p>
                  </div>
                ))}
                {Object.values(provisional).map((seg) => (
                  <div
                    key={`provisional-${seg.source}`}
                    className="flex flex-col gap-0.5 opacity-50"
                  >
                    <span
                      className={`text-[0.65rem] font-semibold uppercase tracking-wider ${
                        seg.source === "system"
                          ? "text-primary"
                          : "text-success"
                      }`}
                    >
                      {seg.source === "system" ? "System" : "Mic"}
                    </span>
                    <p className="text-sm text-neutral-700 dark:text-neutral-300 m-0 leading-snug italic">
                      {seg.text}
                    </p>
                  </div>
                ))}
              </>
            )}
            <div ref={transcriptEndRef} />
          </div>
        </>
      )}

      {error && (
        <div className="bg-red-50 border border-red-200 text-red-700 rounded-lg py-2.5 px-4 text-sm max-w-[360px] text-center dark:bg-danger/10 dark:border-danger/25 dark:text-red-300">
          {error}
        </div>
      )}
    </div>
  );
}

export default RecordingView;
