import { ChartNoAxesColumn, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ButtonGroup, ButtonGroupSeparator } from "@/components/ui/button-group";
import { useAudioLevels } from "@/features/recording";
import { cn } from "@/lib/utils";

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
    <ButtonGroup className={cn(className, "rounded-full")}>
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
          <ButtonGroupSeparator />
          <Button
            aria-label="Stop recording"
            title="Stop recording"
            onClick={(e) => {
              e.stopPropagation();
              void onStopRecording();
            }}
            variant="secondary"
            className="rounded-full"
          >
            <Square className="size-2.5 fill-destructive" strokeWidth={0} />
          </Button>
        </>
      )}
    </ButtonGroup>
  );
}
