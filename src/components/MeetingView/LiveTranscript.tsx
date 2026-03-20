import { type RefObject } from "react";
import { type TranscriptSegment } from "@/hooks/useRecording";
import { SegmentBubble } from "./SegmentBubble";

export function LiveTranscript({
  segments,
  provisional,
  scrollRef,
}: {
  segments: TranscriptSegment[];
  provisional: Record<string, TranscriptSegment>;
  scrollRef: RefObject<HTMLDivElement | null>;
}) {
  const hasContent = segments.length > 0 || Object.keys(provisional).length > 0;

  if (!hasContent) {
    return (
      <p className="text-sm text-neutral-400 text-center mt-8">
        Transcript will appear here...
      </p>
    );
  }

  return (
    <>
      {segments.map((seg, i) => (
        <SegmentBubble key={i} source={seg.source} text={seg.text} />
      ))}
      {Object.values(provisional).map((seg) => (
        <SegmentBubble
          key={`provisional-${seg.source}`}
          source={seg.source}
          text={seg.text}
          provisional
        />
      ))}
      <div ref={scrollRef} />
    </>
  );
}
