import { createContext } from "react";
import type { TranscriptSegment } from "./types";

export interface RecordingContextValue {
  /** True while audio capture is running (not paused). */
  recording: boolean;
  /** True when a meeting session is open but capture is paused (e.g. transcript sheet). */
  paused: boolean;
  meetingId: string | null;
  /** While set, persisted transcript for this meeting may still be catching up after stop. */
  transcriptFinalizingMeetingId: string | null;
  elapsed: number;
  segments: TranscriptSegment[];
  provisional: Record<string, TranscriptSegment>;
  startRecording: (resumeMeetingId?: string | null) => Promise<string>;
  stopRecording: () => Promise<void>;
  pauseRecording: () => Promise<void>;
  resumeRecording: () => Promise<void>;
  /** Replace live transcript (e.g. hydrate from a resumed meeting's saved segments). */
  seedLiveTranscript: (segments: TranscriptSegment[]) => void;
}

export interface AudioLevelContextValue {
  systemLevel: number;
  micLevel: number;
}

export const RecordingContext = createContext<RecordingContextValue | null>(
  null,
);

export const AudioLevelContext = createContext<AudioLevelContextValue | null>(
  null,
);
