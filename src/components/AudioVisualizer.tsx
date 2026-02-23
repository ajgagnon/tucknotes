/** Minimum height for outer bars so they remain visible at silence. */
const MIN_OUTER = 0.15;
/** Minimum height for the center bar (tallest idle state). */
const MIN_CENTER = 0.2;

interface AudioVisualizerProps {
  systemLevel: number;
  micLevel: number;
}

function AudioVisualizer({ systemLevel, micLevel }: AudioVisualizerProps) {
  const bars = [
    Math.max(MIN_OUTER, micLevel),
    Math.max(MIN_CENTER, Math.max(systemLevel, micLevel)),
    Math.max(MIN_OUTER, systemLevel),
  ];

  return (
    <div className="flex items-end gap-[2px] h-4">
      {bars.map((height, i) => (
        <div
          key={i}
          className="w-[3px] rounded-full bg-danger transition-all duration-100 ease-out"
          style={{ height: `${height * 100}%` }}
        />
      ))}
    </div>
  );
}

export default AudioVisualizer;
