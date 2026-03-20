import { useCallback, useEffect, useRef, useState } from "react";
import { Sparkles, Settings2 } from "lucide-react";
import ReactMarkdown from "react-markdown";
import { invoke } from "@tauri-apps/api/core";
import {
  useRecording,
  type TranscriptSegment,
  type AppError,
} from "@/hooks/useRecording";
import { Button } from "@/components/ui/button";
import {
  Sheet,
  SheetContent,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { type MeetingDetail, type MeetingTitleInfo } from "./types";
import { useMeetingSummarization } from "./useMeetingSummarization";
import { ThinkingBlock } from "./ThinkingBlock";
import { LiveTranscript } from "./LiveTranscript";
import { PersistedTranscript } from "./PersistedTranscript";
import { RecordingErrorBanner } from "./RecordingErrorBanner";
import { TranscriptFab } from "./TranscriptFab";

interface MeetingDetailViewProps {
  detail: MeetingDetail;
  isLiveRecording: boolean;
  liveSegments: TranscriptSegment[];
  provisional: Record<string, TranscriptSegment>;
  error: AppError | null;
  onTitleChange?: (info: MeetingTitleInfo) => void;
  /** After starting capture for an existing meeting (resume completed). */
  onRecordingStarted?: (meetingId: string) => void;
}

export function MeetingDetailView({
  detail,
  isLiveRecording,
  liveSegments,
  provisional,
  error,
  onTitleChange,
  onRecordingStarted,
}: MeetingDetailViewProps) {
  const transcriptEndRef = useRef<HTMLDivElement>(null);
  const [transcriptOpen, setTranscriptOpen] = useState(false);
  const {
    recording,
    paused,
    transcriptFinalizingMeetingId,
    pauseRecording,
    resumeRecording,
    stopRecording,
    startRecording,
    seedLiveTranscript,
  } = useRecording();
  const transcriptFinalizing =
    transcriptFinalizingMeetingId === detail.meeting.id;

  const {
    summarizing,
    streamedSummary,
    thinkingText,
    summaryError,
    llmModelReady,
    currentSummary,
    handleSummarize,
  } = useMeetingSummarization(detail.meeting, onTitleChange);

  useEffect(() => {
    if (isLiveRecording && transcriptOpen) {
      transcriptEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [liveSegments, provisional, isLiveRecording, transcriptOpen]);

  useEffect(() => {
    if (!transcriptOpen || !isLiveRecording || !recording || paused) return;
    void pauseRecording();
  }, [transcriptOpen, isLiveRecording, recording, paused, pauseRecording]);

  const handleTranscriptOpenChange = useCallback(
    (open: boolean) => {
      if (!open && isLiveRecording) {
        void resumeRecording();
      }
      setTranscriptOpen(open);
    },
    [isLiveRecording, resumeRecording],
  );

  const handleFooterPrimaryAction = useCallback(async () => {
    try {
      if (recording) {
        await stopRecording();
        setTranscriptOpen(false);
        return;
      }
      if (isLiveRecording) {
        await resumeRecording();
      } else {
        const id = await startRecording(detail.meeting.id);
        onRecordingStarted?.(id);
        seedLiveTranscript(
          detail.segments.map((s) => ({
            text: s.text,
            source: s.source,
            timestamp_ms: s.timestamp_ms,
            is_provisional: false,
          })),
        );
      }
      setTranscriptOpen(false);
    } catch {
      /* error surfaced via context */
    }
  }, [
    detail.meeting.id,
    detail.segments,
    isLiveRecording,
    onRecordingStarted,
    recording,
    resumeRecording,
    seedLiveTranscript,
    startRecording,
    stopRecording,
  ]);

  const summaryBody = summarizing ? (
    <div className="text-sm leading-relaxed">
      {thinkingText && !streamedSummary && (
        <ThinkingBlock text={thinkingText} />
      )}
      {streamedSummary && (
        <div className="prose prose-sm dark:prose-invert max-w-none">
          <ReactMarkdown>{streamedSummary}</ReactMarkdown>
          <span className="inline-block w-1.5 h-4 bg-primary animate-pulse ml-0.5 align-text-bottom rounded-sm" />
        </div>
      )}
    </div>
  ) : currentSummary ? (
    <div className="prose prose-sm dark:prose-invert max-w-none">
      <ReactMarkdown>{currentSummary}</ReactMarkdown>
    </div>
  ) : llmModelReady === false ? (
    <p className="text-sm text-neutral-400 italic">
      Download a summarization model in Settings to enable AI summaries.
    </p>
  ) : (
    <p className="text-sm text-neutral-400 italic">
      Click &ldquo;Summarize&rdquo; to generate an AI summary.
    </p>
  );

  return (
    <div className="flex h-full min-h-0 flex-col p-5">
      {error && <RecordingErrorBanner error={error} />}

      {!isLiveRecording && (
        <div className="mb-4 flex shrink-0 items-center justify-between">
          <div className="flex items-center gap-1.5 text-sm font-medium">
            Summary
            {summarizing && (
              <span className="inline-block size-1.5 animate-pulse rounded-full bg-muted-foreground" />
            )}
          </div>
          <div className="flex items-center gap-2">
            {transcriptFinalizing && (
              <span className="text-xs text-muted-foreground tabular-nums">
                Saving transcript…
              </span>
            )}
            {!summarizing && llmModelReady && !transcriptFinalizing && (
              <Button
                onClick={handleSummarize}
                size="xs"
                variant="outline"
                className="rounded-full"
              >
                <Sparkles className="size-2.5" />
                {currentSummary ? "Resummarize" : "Summarize"}
              </Button>
            )}
          </div>
        </div>
      )}

      <div className="relative flex min-h-0 flex-1 flex-col">
        {isLiveRecording ? (
          <div className="flex flex-1 flex-col items-center justify-center px-4 text-center">
            <p className="text-sm text-muted-foreground">
              {paused
                ? "Recording is paused while the transcript is open. Tap Resume below to continue."
                : "Tap the button below to open the live transcript."}
            </p>
          </div>
        ) : (
          <div className="min-h-0 flex-1 overflow-y-auto">
            {summaryBody}
            {summaryError && !summarizing && (
              <p className="mt-2 text-xs text-red-500 dark:text-red-400">
                {summaryError}
              </p>
            )}
          </div>
        )}

        <TranscriptFab
          className="absolute bottom-1 right-0 z-10"
          open={transcriptOpen}
          onOpenChange={handleTranscriptOpenChange}
          capturing={recording}
        />
      </div>

      <Sheet open={transcriptOpen} onOpenChange={handleTranscriptOpenChange}>
        <SheetContent
          side="right"
          showCloseButton
          className="flex bg-muted flex-col gap-0 rounded-t-2xl p-0"
        >
          <SheetHeader className="shrink-0 border-b px-4 py-3">
            <SheetTitle>Transcript</SheetTitle>
          </SheetHeader>
          <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
            {isLiveRecording ? (
              <div className="flex flex-col gap-3">
                <LiveTranscript
                  segments={liveSegments}
                  provisional={provisional}
                  scrollRef={transcriptEndRef}
                />
              </div>
            ) : (
              <div className="flex flex-col gap-3">
                <PersistedTranscript segments={detail.segments} />
              </div>
            )}
          </div>
          <SheetFooter className="mt-0 shrink-0 flex-row items-center justify-between gap-3 border-t px-4 py-3 sm:flex-row">
            <button
              type="button"
              onClick={() => void handleFooterPrimaryAction()}
              className={
                recording
                  ? "text-sm font-medium text-danger hover:underline"
                  : "text-sm font-medium text-success hover:underline"
              }
            >
              {recording ? "Stop recording" : "Resume"}
            </button>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              className="shrink-0 text-muted-foreground hover:text-foreground"
              aria-label="Open Sound settings (macOS)"
              title="Sound settings"
              onClick={() => {
                invoke("open_sound_settings").catch((e) =>
                  console.error("open_sound_settings:", e),
                );
              }}
            >
              <Settings2 className="size-4" />
            </Button>
          </SheetFooter>
        </SheetContent>
      </Sheet>
    </div>
  );
}
