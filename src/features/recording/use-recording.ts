import { useContext } from "react";
import {
  RecordingContext,
  type RecordingContextValue,
} from "./recording-context";

export function useRecording(): RecordingContextValue {
  const ctx = useContext(RecordingContext);
  if (!ctx)
    throw new Error("useRecording must be used within RecordingProvider");
  return ctx;
}
