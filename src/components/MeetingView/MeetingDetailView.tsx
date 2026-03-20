import { useEffect, useRef } from "react";
import { Sparkles } from "lucide-react";
import ReactMarkdown from "react-markdown";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  useRecording,
  type TranscriptSegment,
  type AppError,
} from "@/hooks/useRecording";
import { Button } from "@/components/ui/button";
import { type MeetingDetail, type MeetingTitleInfo } from "./types";
import { useMeetingSummarization } from "./useMeetingSummarization";
import { ThinkingBlock } from "./ThinkingBlock";
import { LiveTranscript } from "./LiveTranscript";
import { PersistedTranscript } from "./PersistedTranscript";
import { RecordingErrorBanner } from "./RecordingErrorBanner";

interface MeetingDetailViewProps {
  detail: MeetingDetail;
  isLiveRecording: boolean;
  liveSegments: TranscriptSegment[];
  provisional: Record<string, TranscriptSegment>;
  error: AppError | null;
  onTitleChange?: (info: MeetingTitleInfo) => void;
}

export function MeetingDetailView({
  detail,
  isLiveRecording,
  liveSegments,
  provisional,
  error,
  onTitleChange,
}: MeetingDetailViewProps) {
  const transcriptEndRef = useRef<HTMLDivElement>(null);
  const { transcriptFinalizingMeetingId } = useRecording();
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
    if (isLiveRecording) {
      transcriptEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [liveSegments, provisional, isLiveRecording]);

  return (
    <div className="flex flex-col h-full p-5">
      {error && <RecordingErrorBanner error={error} />}

      {isLiveRecording ? (
        <div className="flex-1 overflow-y-auto flex flex-col gap-3 ">
          <LiveTranscript
            segments={liveSegments}
            provisional={provisional}
            scrollRef={transcriptEndRef}
          />
        </div>
      ) : (
        <Tabs defaultValue="summary" className="flex flex-col flex-1 min-h-0">
          <div className="flex items-center justify-between">
            <TabsList>
              <TabsTrigger
                value="summary"
                className="flex items-center gap-1.5"
              >
                Summary
                {summarizing && (
                  <span className="inline-block w-1.5 h-1.5 rounded-full bg-muted-foreground animate-pulse" />
                )}
              </TabsTrigger>
              <TabsTrigger value="transcript">Transcript</TabsTrigger>
            </TabsList>
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

          <TabsContent value="summary" className="flex-1 overflow-y-auto mt-4">
            {summarizing ? (
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
                Download a summarization model in Settings to enable AI
                summaries.
              </p>
            ) : (
              <p className="text-sm text-neutral-400 italic">
                Click &ldquo;Summarize&rdquo; to generate an AI summary.
              </p>
            )}

            {summaryError && !summarizing && (
              <p className="text-xs text-red-500 dark:text-red-400 mt-2">
                {summaryError}
              </p>
            )}
          </TabsContent>

          <TabsContent
            value="transcript"
            className="flex-1 overflow-y-auto mt-4"
          >
            <div className="flex flex-col gap-3">
              <PersistedTranscript segments={detail.segments} />
            </div>
          </TabsContent>
        </Tabs>
      )}
    </div>
  );
}
