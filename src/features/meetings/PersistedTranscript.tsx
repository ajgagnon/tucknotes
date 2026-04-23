import { forwardRef, useImperativeHandle, useRef } from "react";
import { type SegmentRow, type TranscriptScrollHandle } from "./types";
import { SegmentBubble } from "./SegmentBubble";
import { scrollAndHighlight } from "./transcriptScroll";

export const PersistedTranscript = forwardRef<
  TranscriptScrollHandle,
  { segments: SegmentRow[] }
>(function PersistedTranscript({ segments }, ref) {
  const containerRef = useRef<HTMLDivElement>(null);

  useImperativeHandle(ref, () => ({
    scrollToTimeMs(ms: number) {
      scrollAndHighlight(containerRef.current, ms);
    },
  }));

  if (segments.length === 0) {
    return (
      <p className="text-sm text-neutral-400 text-center mt-8">
        No transcript segments recorded.
      </p>
    );
  }

  return (
    <div ref={containerRef} className="flex flex-col gap-3">
      {segments.map((seg) => (
        <div key={seg.id} data-timestamp-ms={seg.timestamp_ms}>
          <SegmentBubble source={seg.source} text={seg.text} />
        </div>
      ))}
    </div>
  );
});
