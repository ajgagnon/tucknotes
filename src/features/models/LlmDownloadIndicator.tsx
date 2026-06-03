import { CheckCircle2, DownloadCloud } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { formatSize } from "./types";
import { useLlmDownloadProgress } from "./use-llm-download-progress";

export function LlmDownloadIndicator() {
  const status = useLlmDownloadProgress();
  if (!status) return null;

  return (
    <Tooltip>
      <TooltipTrigger
        render={<div className="px-2 py-1.5 text-xs cursor-default" />}
      >
        <div className="flex items-center gap-1.5 mb-1 text-muted-foreground">
          {status.done ? (
            <CheckCircle2 className="size-3 shrink-0 text-primary" />
          ) : (
            <DownloadCloud className="size-3 shrink-0" />
          )}
          <span className="truncate flex-1 min-w-0">
            {status.done ? "Download complete" : "Model downloading..."}
          </span>
          <span className="tabular-nums shrink-0">
            {Math.round(status.percent)}%
          </span>
        </div>
        <div className="h-1 w-full rounded-full bg-muted overflow-hidden">
          <div
            className="h-full bg-primary rounded-full transition-all duration-300 ease-out"
            style={{ width: `${status.percent}%` }}
          />
        </div>
        {!status.done && status.totalBytes > 0 && (
          <p className="mt-1 text-[11px] text-muted-foreground tabular-nums">
            {formatSize(status.downloadedBytes)} /{" "}
            {formatSize(status.totalBytes)}
          </p>
        )}
      </TooltipTrigger>
      <TooltipContent side="right" className="max-w-[260px] text-pretty">
        TuckNotes is downloading a language model that runs locally on your
        device to summarize your meetings.
      </TooltipContent>
    </Tooltip>
  );
}
