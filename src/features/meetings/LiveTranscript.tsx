import { forwardRef, useImperativeHandle, useRef } from "react";
import { type TranscriptSegment } from "@/features/recording";
import { type TranscriptScrollHandle } from "./types";
import { SegmentBubble } from "./SegmentBubble";
import { scrollAndHighlight } from "./transcriptScroll";

export const LiveTranscript = forwardRef<
  TranscriptScrollHandle,
  {
    segments: TranscriptSegment[];
    provisional: Record<string, TranscriptSegment>;
    scrollRef: React.RefObject<HTMLDivElement | null>;
  }
>(function LiveTranscript({ segments, provisional, scrollRef }, ref) {
  const containerRef = useRef<HTMLDivElement>(null);

  useImperativeHandle(ref, () => ({
    scrollToTimeMs(ms: number) {
      scrollAndHighlight(containerRef.current, ms);
    },
  }));

  const hasContent = segments.length > 0 || Object.keys(provisional).length > 0;

  if (!hasContent) {
    return (
      <p className="text-sm text-neutral-400 text-center mt-8">
        Transcript will appear here...
      </p>
    );
  }

  return (
    <div ref={containerRef}>
      <div className="grid grid-cols-[auto_auto_1fr] gap-x-6">
        {segments.map((seg, i) => (
          <SegmentBubble
            key={i}
            timestampMs={seg.timestamp_ms}
            source={seg.source}
            text={seg.text}
          />
        ))}
        {Object.values(provisional).map((seg) => (
          <SegmentBubble
            key={`provisional-${seg.source}`}
            timestampMs={seg.timestamp_ms}
            source={seg.source}
            text={seg.text}
            provisional
          />
        ))}
      </div>
      <div ref={scrollRef} />
    </div>
  );
});
