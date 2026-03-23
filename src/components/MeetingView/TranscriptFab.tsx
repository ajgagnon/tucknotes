import { AudioLines, ChevronDown, ChevronUp, Square } from "lucide-react";
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

/** Pill control: AudioLines when idle, live bars while capturing + optional stop + chevron for transcript sheet. */
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
        "flex h-11 shrink-0 items-center rounded-full border border-border bg-muted shadow-md transition-[box-shadow,transform] hover:shadow-lg",
        className,
      )}
    >
      <button
        type="button"
        aria-label={open ? "Close transcript" : "Open transcript"}
        aria-expanded={open}
        onClick={() => onOpenChange(!open)}
        className="flex h-full min-w-0 flex-1 items-center justify-center gap-1.5 rounded-full px-3.5 transition-[transform] active:scale-95"
      >
        {capturing ? (
          <div className="flex h-4 items-end gap-[2px]">
            {bars.map((height, i) => (
              <div
                key={i}
                className="w-[3px] rounded-full bg-muted-foreground transition-all duration-100 ease-out"
                style={{ height: `${height * 100}%` }}
              />
            ))}
          </div>
        ) : (
          <AudioLines
            className="size-4 shrink-0 text-muted-foreground"
            strokeWidth={2}
          />
        )}
        {open ? (
          <ChevronDown
            className="size-3.5 text-muted-foreground"
            strokeWidth={2}
          />
        ) : (
          <ChevronUp
            className="size-3.5 text-muted-foreground"
            strokeWidth={2}
          />
        )}
      </button>
      {onStopRecording && (
        <>
          <div className="h-5 w-px shrink-0 bg-border" aria-hidden />
          <button
            type="button"
            aria-label="Stop recording"
            title="Stop recording"
            onClick={(e) => {
              e.stopPropagation();
              void onStopRecording();
            }}
            className="flex h-full shrink-0 items-center justify-center rounded-r-full px-3 text-danger transition-colors hover:bg-danger/10 active:scale-95"
          >
            <Square className="size-3 fill-current" strokeWidth={0} />
          </button>
        </>
      )}
    </div>
  );
}
