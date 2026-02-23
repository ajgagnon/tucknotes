interface AudioVisualizerProps {
  systemLevel: number;
  micLevel: number;
}

function AudioVisualizer({ systemLevel, micLevel }: AudioVisualizerProps) {
  const bars = [
    Math.max(0.15, micLevel),
    Math.max(0.2, Math.max(systemLevel, micLevel)),
    Math.max(0.15, systemLevel),
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
