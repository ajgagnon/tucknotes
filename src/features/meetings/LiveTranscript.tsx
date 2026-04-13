import { forwardRef, useImperativeHandle, useRef } from "react";
import { type TranscriptSegment } from "@/features/recording";
import { type TranscriptScrollHandle } from "./types";
import { SegmentBubble } from "./SegmentBubble";

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
      const root = containerRef.current;
      if (!root) return;
      const rows = [...root.querySelectorAll("[data-timestamp-ms]")] as HTMLElement[];
      const sorted = rows
        .map((el) => ({
          el,
          t: parseInt(el.getAttribute("data-timestamp-ms") ?? "0", 10),
        }))
        .sort((a, b) => a.t - b.t);
      let best: HTMLElement | null = null;
      for (const { el, t } of sorted) {
        if (t <= ms) best = el;
        else break;
      }
      best?.scrollIntoView({ behavior: "smooth", block: "nearest" });
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
    <div ref={containerRef} className="flex flex-col gap-3">
      {segments.map((seg, i) => (
        <div key={i} data-timestamp-ms={seg.timestamp_ms}>
          <SegmentBubble source={seg.source} text={seg.text} />
        </div>
      ))}
      {Object.values(provisional).map((seg) => (
        <div key={`provisional-${seg.source}`} data-timestamp-ms={seg.timestamp_ms}>
          <SegmentBubble source={seg.source} text={seg.text} provisional />
        </div>
      ))}
      <div ref={scrollRef} />
    </div>
  );
});
