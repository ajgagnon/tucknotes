import { type RefObject } from "react";
import { Settings2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useRecording } from "@/features/recording";
import type { MeetingDetail, TranscriptScrollHandle } from "./types";
import { LiveTranscript } from "./LiveTranscript";
import { PersistedTranscript } from "./PersistedTranscript";
import { TranscriptActionsMenu } from "./TranscriptActionsMenu";
import { type TranscriptLine } from "./exportTranscript";

/** The transcript tab: live or persisted segments plus the actions footer. */
export function TranscriptPanel({
  meeting,
  persistedSegments,
  isLiveRecording,
  scrollHandleRef,
  endRef,
}: {
  meeting: MeetingDetail["meeting"];
  persistedSegments: MeetingDetail["segments"];
  isLiveRecording: boolean;
  scrollHandleRef: RefObject<TranscriptScrollHandle | null>;
  endRef: RefObject<HTMLDivElement | null>;
}) {
  const {
    recording,
    paused,
    segments: liveSegments,
    provisional,
  } = useRecording();

  // Segments backing the transcript copy/export actions. During a live
  // recording the committed live segments are authoritative; otherwise the
  // persisted segments from the loaded meeting detail are used.
  const transcriptLines: TranscriptLine[] = isLiveRecording
    ? liveSegments
    : persistedSegments;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {isLiveRecording ? (
          <div className="flex flex-col gap-3">
            <LiveTranscript
              ref={scrollHandleRef}
              segments={liveSegments}
              provisional={provisional}
              scrollRef={endRef}
            />
          </div>
        ) : (
          <div className="flex flex-col gap-3">
            <PersistedTranscript
              ref={scrollHandleRef}
              segments={persistedSegments}
            />
          </div>
        )}
      </div>
      <div className="mt-0 shrink-0 flex flex-row items-center gap-2 border-t px-4 py-3">
        {!isLiveRecording && (recording || paused) && (
          <p className="text-xs text-muted-foreground">
            Another meeting is being recorded.
          </p>
        )}
        <div className="flex items-center gap-2">
          <TranscriptActionsMenu
            meeting={meeting}
            segments={transcriptLines}
            className="shrink-0"
          />
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Open Sound settings (macOS)"
                  onClick={() => {
                    invoke("open_sound_settings").catch((e) =>
                      console.error("open_sound_settings:", e),
                    );
                  }}
                />
              }
            >
              <Settings2 className="size-4" />
            </TooltipTrigger>
            <TooltipContent>Sound settings</TooltipContent>
          </Tooltip>
        </div>
      </div>
    </div>
  );
}
