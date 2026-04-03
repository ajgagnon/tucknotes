import { cn } from "@/lib/utils";

/** Minimum height for outer bars so they remain visible at silence. */
const MIN_OUTER = 0.15;
/** Minimum height for the center bar (tallest idle state). */
const MIN_CENTER = 0.2;

interface AudioVisualizerProps {
  systemLevel: number;
  micLevel: number;
  /** Applied to each bar (default: recording indicator red). */
  barClassName?: string;
}

function AudioVisualizer({
  systemLevel,
  micLevel,
  barClassName = "bg-danger",
}: AudioVisualizerProps) {
  const bars = [
    Math.max(MIN_OUTER, micLevel),
    Math.max(MIN_CENTER, Math.max(systemLevel, micLevel)),
    Math.max(MIN_OUTER, systemLevel),
  ];

  return (
    <div className="flex h-4 shrink-0 items-end gap-[2px]">
      {bars.map((height, i) => (
        <div
          key={i}
          className={cn(
            "w-[3px] rounded-full transition-all duration-100 ease-out",
            barClassName,
          )}
          style={{ height: `${height * 100}%` }}
        />
      ))}
    </div>
  );
}

export default AudioVisualizer;
