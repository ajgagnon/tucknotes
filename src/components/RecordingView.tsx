import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import "./RecordingView.css";

interface AudioChunkEvent {
  sample_count: number;
  rms: number;
  source: string;
  timestamp: number;
}

function formatTime(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  const mm = String(m).padStart(2, "0");
  const ss = String(s).padStart(2, "0");
  if (h > 0) return `${h}:${mm}:${ss}`;
  return `${mm}:${ss}`;
}

function RecordingView() {
  const [recording, setRecording] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [systemChunks, setSystemChunks] = useState(0);
  const [micChunks, setMicChunks] = useState(0);
  const [systemLevel, setSystemLevel] = useState(0);
  const [micLevel, setMicLevel] = useState(0);

  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const unlistenRef = useRef<UnlistenFn | null>(null);

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

      // dB-based meter: maps -60dB (near-silence) → 0, 0dB (full scale) → 1
      const db = rms > 0 ? 20 * Math.log10(rms) : -100;
      const level = Math.max(0, Math.min(1, (db + 60) / 60));

      // Fast attack, slow decay: jump to peaks instantly, fade out gradually
      if (source === "system") {
        setSystemChunks((c) => c + 1);
        setSystemLevel((prev) => (level > prev ? level : prev * 0.92 + level * 0.08));
      } else {
        setMicChunks((c) => c + 1);
        setMicLevel((prev) => (level > prev ? level : prev * 0.92 + level * 0.08));
      }
    }).then((unlisten) => {
      unlistenRef.current = unlisten;
    });

    return () => {
      mounted = false;
      unlistenRef.current?.();
    };
  }, []);

  useEffect(() => {
    return clearTimer;
  }, [clearTimer]);

  // Decay audio levels when no new data arrives
  useEffect(() => {
    if (!recording) return;
    const decay = setInterval(() => {
      setSystemLevel((l) => l * 0.85);
      setMicLevel((l) => l * 0.85);
    }, 150);
    return () => clearInterval(decay);
  }, [recording]);

  return (
    <div className="recording-view">
      {recording && <p className="recording-timer">{formatTime(elapsed)}</p>}

      <button
        className={`record-btn ${recording ? "recording" : ""}`}
        onClick={handleToggle}
        title={recording ? "Stop recording" : "Start recording"}
      >
        <span className="record-btn-inner" />
      </button>

      <div className={`recording-status ${recording ? "status-active" : "status-idle"}`}>
        <h2>{recording ? "Recording" : "Ready"}</h2>
        <p>
          {recording
            ? "Capturing system audio & microphone"
            : "Tap to start recording your meeting"}
        </p>
      </div>

      {recording && (
        <>
          <div className="audio-levels">
            <div className="audio-level-row">
              <span className="audio-level-label">System</span>
              <div className="audio-level-track">
                <div
                  className="audio-level-fill system"
                  style={{ width: `${systemLevel * 100}%` }}
                />
              </div>
            </div>
            <div className="audio-level-row">
              <span className="audio-level-label">Mic</span>
              <div className="audio-level-track">
                <div
                  className="audio-level-fill mic"
                  style={{ width: `${micLevel * 100}%` }}
                />
              </div>
            </div>
          </div>

          <div className="chunk-stats">
            <div className="chunk-stat">
              <span className="chunk-stat-value system">{systemChunks}</span>
              <span className="chunk-stat-label">System chunks</span>
            </div>
            <div className="chunk-stat">
              <span className="chunk-stat-value mic">{micChunks}</span>
              <span className="chunk-stat-label">Mic chunks</span>
            </div>
          </div>
        </>
      )}

      {error && <div className="recording-error">{error}</div>}
    </div>
  );
}

export default RecordingView;
