import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from "@/components/ui/hover-card";
import { cn } from "@/lib/utils";

import type { SearchHit } from "./use-chat-stream";

export type CitationChipProps = {
  /**
   * The citation number the rehype plugin parsed out of `[N]`. Passed via
   * `data-citation` on the `<cite>` element Streamdown renders.
   */
  "data-citation"?: string;
  sources: SearchHit[];
  onOpenMeeting?: (meetingId: string) => void;
  children?: React.ReactNode;
};

export function CitationChip({
  "data-citation": dataCitation,
  sources,
  onOpenMeeting,
  children,
}: CitationChipProps) {
  const n = Number.parseInt(dataCitation ?? "", 10);
  const source = Number.isFinite(n) ? sources[n - 1] : undefined;

  // Out-of-range or missing source → render the original `[N]` literally so
  // the user sees what the model emitted instead of a broken chip.
  if (!source) {
    return <>{children}</>;
  }

  return (
    <HoverCard>
      <HoverCardTrigger
        delay={120}
        render={
          <button
            type="button"
            onClick={() => onOpenMeeting?.(source.meeting_id)}
            className={cn(
              "mx-0.5 inline-flex items-baseline rounded px-1 text-[0.7em] font-medium leading-none align-super",
              "bg-blue-500/10 text-blue-700 hover:bg-blue-500/20 dark:text-blue-300",
              "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-blue-500",
            )}
            aria-label={`Source ${n}: ${source.meeting_title ?? "Untitled meeting"}`}
          >
            {n}
          </button>
        }
      />
      <HoverCardContent side="top" className="w-72">
        <div className="flex flex-col gap-1.5 text-xs">
          <div className="flex items-center gap-1.5">
            <span className="min-w-0 flex-1 truncate font-medium text-foreground">
              {source.meeting_title || "Untitled meeting"}
            </span>
            <span
              className={cn(
                "shrink-0 rounded px-1 py-0.5 text-[10px] uppercase tracking-wide",
                source.kind === "summary"
                  ? "bg-blue-500/10 text-blue-600 dark:text-blue-400"
                  : "bg-muted text-muted-foreground",
              )}
            >
              {source.kind}
            </span>
          </div>
          <div
            className="text-muted-foreground [&_mark]:rounded [&_mark]:bg-yellow-200/60 [&_mark]:px-0.5 [&_mark]:text-foreground dark:[&_mark]:bg-yellow-300/30"
            dangerouslySetInnerHTML={{ __html: source.snippet }}
          />
          {onOpenMeeting && (
            <button
              type="button"
              onClick={() => onOpenMeeting(source.meeting_id)}
              className="mt-0.5 self-start text-xs font-medium text-blue-600 hover:underline dark:text-blue-400"
            >
              Open meeting →
            </button>
          )}
        </div>
      </HoverCardContent>
    </HoverCard>
  );
}
