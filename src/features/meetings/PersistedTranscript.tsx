import { forwardRef, useImperativeHandle, useRef } from "react";
import { type SegmentRow, type TranscriptScrollHandle } from "./types";
import { SegmentBubble } from "./SegmentBubble";

export const PersistedTranscript = forwardRef<
  TranscriptScrollHandle,
  { segments: SegmentRow[] }
>(function PersistedTranscript({ segments }, ref) {
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
