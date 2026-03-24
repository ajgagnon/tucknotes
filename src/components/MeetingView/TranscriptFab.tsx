import { ChartNoAxesColumn, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useAudioLevels } from "@/hooks/useRecording";

const MIN_OUTER = 0.15;
const MIN_CENTER = 0.2;

interface TranscriptFabProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** True while mic/system capture is running for this meeting (not paused). */
  capturing: boolean;
  /** Stops recording without toggling the transcript sheet. */
  onStopRecording?: () => void | Promise<void>;
  className?: string;
}

/** Pill control (h-8, aligned with document tabs): AudioLines when idle, live bars while capturing + optional stop + chevron for transcript sheet. */
export function TranscriptFab({
  open,
  onOpenChange,
  capturing,
  onStopRecording,
  className,
}: TranscriptFabProps) {
  const { systemLevel, micLevel } = useAudioLevels();
  const bars = [
    Math.max(MIN_OUTER, micLevel),
    Math.max(MIN_CENTER, Math.max(systemLevel, micLevel)),
    Math.max(MIN_OUTER, systemLevel),
  ];

  return (
    <div
      className={cn(
        "flex h-9 shrink-0 items-center rounded-full border border-muted bg-muted shadow-sm transition-[box-shadow,transform] hover:shadow-md",
        className,
      )}
    >
      <Button
        variant="secondary"
        aria-label={open ? "Close transcript" : "Open transcript"}
        aria-expanded={open}
        onClick={() => onOpenChange(!open)}
        className="rounded-full text-xs"
      >
        {capturing ? (
          <div className="flex h-2.5 items-end gap-px">
            {bars.map((height, i) => (
              <div
                key={i}
                className="w-0.5 rounded-full bg-muted-foreground transition-all duration-100 ease-out"
                style={{ height: `${height * 100}%` }}
              />
            ))}
          </div>
        ) : (
          <>
          <span>Transcript</span>
          <ChartNoAxesColumn
            className="size-3 shrink-0 text-muted-foreground"
            strokeWidth={2}
          />
          </>
        )}
      </Button>
      {onStopRecording && (
        <>
          <div
            className="h-3.5 w-px shrink-0 bg-border"
            aria-hidden
          />
          <button
            type="button"
            aria-label="Stop recording"
            title="Stop recording"
            onClick={(e) => {
              e.stopPropagation();
              void onStopRecording();
            }}
            className="flex h-full shrink-0 items-center justify-center rounded-r-full px-2 text-danger transition-colors hover:bg-danger/10 active:scale-95"
          >
            <Square className="size-2.5 fill-current" strokeWidth={0} />
          </button>
        </>
      )}
    </div>
  );
}
