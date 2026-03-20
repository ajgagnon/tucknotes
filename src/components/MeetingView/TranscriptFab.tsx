import { AudioLines, ChevronDown, ChevronUp } from "lucide-react";
import { cn } from "@/lib/utils";
import { useAudioLevels } from "@/hooks/useRecording";

const MIN_OUTER = 0.15;
const MIN_CENTER = 0.2;

interface TranscriptFabProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** True while mic/system capture is running (not paused / post-meeting). */
  capturing: boolean;
  className?: string;
}

/** Pill control: AudioLines when idle, live bars while capturing + chevron for transcript sheet. */
export function TranscriptFab({
  open,
  onOpenChange,
  capturing,
  className,
}: TranscriptFabProps) {
  const { systemLevel, micLevel } = useAudioLevels();
  const bars = [
    Math.max(MIN_OUTER, micLevel),
    Math.max(MIN_CENTER, Math.max(systemLevel, micLevel)),
    Math.max(MIN_OUTER, systemLevel),
  ];

  return (
    <button
      type="button"
      aria-label={open ? "Close transcript" : "Open transcript"}
      aria-expanded={open}
      onClick={() => onOpenChange(!open)}
      className={cn(
        "flex h-11 shrink-0 items-center justify-center gap-1.5 rounded-full border border-border bg-muted px-3.5 shadow-md transition-[box-shadow,transform] hover:shadow-lg active:scale-95",
        className,
      )}
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
        <AudioLines className="size-4 shrink-0 text-muted-foreground" strokeWidth={2} />
      )}
      {open ? (
        <ChevronDown className="size-3.5 text-muted-foreground" strokeWidth={2} />
      ) : (
        <ChevronUp className="size-3.5 text-muted-foreground" strokeWidth={2} />
      )}
    </button>
  );
}
