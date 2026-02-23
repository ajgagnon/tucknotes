import { useEffect, useRef } from "react";
import { Mic } from "lucide-react";
import { useRecording } from "@/hooks/useRecording";

function MeetingView() {
  const { recording, segments, provisional, error } = useRecording();
  const transcriptEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    transcriptEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [segments, provisional]);

  if (!recording && segments.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <Mic className="w-12 h-12 text-muted-foreground mb-4" />
        <h1 className="text-xl font-semibold mb-2">No Active Meeting</h1>
        <p className="text-sm text-muted-foreground">
          Click "Start Recording" to begin capturing your meeting.
        </p>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col p-6">
      {error && (
        <div className="bg-red-50 border border-red-200 text-red-700 rounded-lg py-2.5 px-4 text-sm mb-4 dark:bg-danger/10 dark:border-danger/25 dark:text-red-300">
          {error}
        </div>
      )}

      <div className="flex-1 overflow-y-auto flex flex-col gap-3">
        {segments.length === 0 && Object.keys(provisional).length === 0 ? (
          <p className="text-sm text-muted-foreground text-center mt-8">
            Transcript will appear here...
          </p>
        ) : (
          <>
            {segments.map((seg, i) => (
              <div key={i} className="flex flex-col gap-0.5">
                <span
                  className={`text-xs font-semibold uppercase tracking-wider ${
                    seg.source === "system" ? "text-primary" : "text-success"
                  }`}
                >
                  {seg.source === "system" ? "Speaker" : "You"}
                </span>
                <p className="text-sm text-foreground leading-relaxed m-0">
                  {seg.text}
                </p>
              </div>
            ))}
            {Object.values(provisional).map((seg) => (
              <div
                key={`provisional-${seg.source}`}
                className="flex flex-col gap-0.5 opacity-50"
              >
                <span
                  className={`text-xs font-semibold uppercase tracking-wider ${
                    seg.source === "system" ? "text-primary" : "text-success"
                  }`}
                >
                  {seg.source === "system" ? "Speaker" : "You"}
                </span>
                <p className="text-sm text-foreground leading-relaxed m-0 italic">
                  {seg.text}
                </p>
              </div>
            ))}
          </>
        )}
        <div ref={transcriptEndRef} />
      </div>
    </div>
  );
}

export default MeetingView;
