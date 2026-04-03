import { useContext } from "react";
import {
  AudioLevelContext,
  type AudioLevelContextValue,
} from "./recording-context";

export function useAudioLevels(): AudioLevelContextValue {
  const ctx = useContext(AudioLevelContext);
  if (!ctx)
    throw new Error("useAudioLevels must be used within RecordingProvider");
  return ctx;
}
