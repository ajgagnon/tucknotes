import {
  useContext,
  useState,
  useEffect,
  useRef,
  useMemo,
  type ReactNode,
} from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { rmsToLevel, smoothLevel } from "@/lib/audio-level";
import { RecordingContext, AudioLevelContext } from "./recording-context";

interface AudioChunkEvent {
  sample_count: number;
  rms: number;
  source: string;
  timestamp: number;
}

export function AudioLevelProvider({ children }: { children: ReactNode }) {
  const recordingCtx = useContext(RecordingContext);
  if (!recordingCtx) {
    throw new Error("AudioLevelProvider must be inside RecordingProviderInner");
  }
  const { recording, paused } = recordingCtx;
  const levelsActive = recording && !paused;
  const [systemLevel, setSystemLevel] = useState(0);
  const [micLevel, setMicLevel] = useState(0);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  useEffect(() => {
    if (!levelsActive) {
      setSystemLevel(0);
      setMicLevel(0);
    }
  }, [levelsActive]);

  useEffect(() => {
    if (!levelsActive) return;
    const decay = setInterval(() => {
      setSystemLevel((l) => l * 0.85);
      setMicLevel((l) => l * 0.85);
    }, 150);
    return () => clearInterval(decay);
  }, [levelsActive]);

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
