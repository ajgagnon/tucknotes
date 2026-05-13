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
    <div
      ref={containerRef}
      className="grid grid-cols-[auto_auto_1fr] gap-x-6"
    >
      {segments.map((seg) => (
        <SegmentBubble
          key={seg.id}
          timestampMs={seg.timestamp_ms}
          source={seg.source}
          text={seg.text}
        />
      ))}
    </div>
  );
});
