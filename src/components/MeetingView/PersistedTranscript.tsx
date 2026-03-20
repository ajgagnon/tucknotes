import { type SegmentRow } from "./types";
import { SegmentBubble } from "./SegmentBubble";

export function PersistedTranscript({ segments }: { segments: SegmentRow[] }) {
  if (segments.length === 0) {
    return (
      <p className="text-sm text-neutral-400 text-center mt-8">
        No transcript segments recorded.
      </p>
    );
  }

  return (
    <>
      {segments.map((seg) => (
        <SegmentBubble key={seg.id} source={seg.source} text={seg.text} />
      ))}
    </>
  );
}
