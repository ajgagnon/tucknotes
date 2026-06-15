import { Play, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { useAudioLevels } from "@/features/recording";
import { cn } from "@/lib/utils";

const MIN_OUTER = 0.15;
const MIN_CENTER = 0.2;

interface TranscriptFabProps {
  /** True while mic/system capture is running for this meeting (not paused). */
  capturing: boolean;
  /** Resumes (or starts) recording for this meeting. Omit when resume is unavailable. */
  onResume?: () => void | Promise<void>;
  /** Stops recording. */
  onStopRecording?: () => void | Promise<void>;
  className?: string;
}

/** Pill control (h-8, aligned with document tabs): waveform while capturing, otherwise a Resume action; optional stop button on the right. */
export function TranscriptFab({
  capturing,
  onResume,
  onStopRecording,
  className,
}: TranscriptFabProps) {
  const { systemLevel, micLevel } = useAudioLevels();
  const bars = [
    Math.max(MIN_OUTER, micLevel),
    Math.max(MIN_CENTER, Math.max(systemLevel, micLevel)),
    Math.max(MIN_OUTER, systemLevel),
  ];

  if (!capturing && !onResume && !onStopRecording) {
    return null;
  }

  const mainButton = (
    <Button
      variant="secondary"
      aria-label={capturing ? "Recording" : "Resume recording"}
      disabled={!capturing && !onResume}
      onClick={capturing ? undefined : () => void onResume?.()}
      className={cn(
        "text-xs",
        onStopRecording ? "rounded-l-full rounded-r-none" : "rounded-full",
        !onStopRecording && className,
      )}
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
          <Play
            className="size-3 shrink-0 text-muted-foreground"
            strokeWidth={2}
          />
          <span>Resume</span>
        </>
      )}
    </Button>
  );

  if (!onStopRecording) {
    return mainButton;
  }

  return (
    <div className={cn("flex w-fit items-stretch", className)}>
      {mainButton}
      <Separator orientation="vertical" className="bg-input" />
      <Button
        aria-label="Stop recording"
        title="Stop recording"
        onClick={(e) => {
          e.stopPropagation();
          void onStopRecording();
        }}
        variant="secondary"
        className="rounded-l-none rounded-r-full"
      >
        <Square className="size-2.5 fill-danger" strokeWidth={0} />
      </Button>
    </div>
  );
}
