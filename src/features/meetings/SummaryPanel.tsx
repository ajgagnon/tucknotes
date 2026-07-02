import { Sparkles } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useLlmDownloadProgress } from "@/features/models";
import type { SummarySection } from "./types";
import { SectionStream } from "./SectionStream";
import { StreamingSummaryToolbarPlaceholder } from "./StreamingSummaryToolbarPlaceholder";

/**
 * The summary tab when there is no editable summary document to show:
 * skeleton while the transcript finalizes, the streaming section view while
 * summarizing, the rendered summary, or the call-to-action / model-download
 * states.
 */
export function SummaryPanel({
  showSkeleton,
  summarizing,
  sections,
  currentSummary,
  llmModelReady,
  onSummarize,
}: {
  showSkeleton: boolean;
  summarizing: boolean;
  sections: SummarySection[];
  currentSummary: string | null;
  llmModelReady: boolean | null;
  onSummarize: () => void;
}) {
  const llmDownload = useLlmDownloadProgress();

  if (showSkeleton) {
    return (
      <div className="space-y-4 p-5" aria-busy="true" aria-live="polite">
        <Skeleton className="h-5 w-1/3" />
        <div className="space-y-2">
          <Skeleton className="h-4 w-full" />
          <Skeleton className="h-4 w-11/12" />
          <Skeleton className="h-4 w-4/5" />
        </div>
        <Skeleton className="h-5 w-1/4" />
        <div className="space-y-2">
          <Skeleton className="h-4 w-full" />
          <Skeleton className="h-4 w-5/6" />
        </div>
      </div>
    );
  }

  if (summarizing) {
    return (
      <div className="simple-editor-wrapper meeting-summary-prose">
        <StreamingSummaryToolbarPlaceholder />
        <div className="simple-editor-content">
          <SectionStream sections={sections} />
        </div>
      </div>
    );
  }

  if (currentSummary) {
    return (
      <div className="simple-editor-wrapper meeting-summary-prose">
        <StreamingSummaryToolbarPlaceholder />
        <div className="simple-editor-content">
          <div
            className="tiptap ProseMirror simple-editor"
            style={{ whiteSpace: "normal" }}
          >
            <ReactMarkdown remarkPlugins={[remarkGfm]}>
              {currentSummary}
            </ReactMarkdown>
          </div>
        </div>
      </div>
    );
  }

  if (llmModelReady === true) {
    return (
      <div className="flex flex-col items-start gap-3 p-5">
        <p className="text-sm text-neutral-400 italic">
          Generate an AI summary of this meeting.
        </p>
        <Button variant="outline" size="sm" onClick={onSummarize}>
          <Sparkles className="size-2.5" />
          Summarize
        </Button>
      </div>
    );
  }

  if (llmDownload) {
    return (
      <div className="flex flex-col gap-2 p-5 text-sm text-neutral-400">
        <span className="italic">
          {llmDownload.done
            ? "Finishing model download…"
            : `Downloading summarization model… ${Math.round(llmDownload.percent)}%`}
        </span>
        <div className="h-1 w-full max-w-xs overflow-hidden rounded-full bg-muted">
          <div
            className="h-full rounded-full bg-primary transition-all duration-300 ease-out"
            style={{ width: `${llmDownload.percent}%` }}
          />
        </div>
      </div>
    );
  }

  return (
    <p className="text-sm text-neutral-400 italic p-5">
      Download a summarization model in Settings to enable AI summaries.
    </p>
  );
}
