import { formatTranscriptTimestamp } from "@/lib/format-time";

export function SegmentBubble({
  timestampMs,
  source,
  text,
  provisional,
}: {
  timestampMs: number;
  source: string;
  text: string;
  provisional?: boolean;
}) {
  return (
    <div
      data-timestamp-ms={timestampMs}
      className={`col-span-3 grid grid-cols-subgrid items-baseline py-3  ${
        provisional ? "opacity-50" : ""
      }`}
    >
      <span className="font-mono text-xs tabular-nums text-muted-foreground">
        {formatTranscriptTimestamp(timestampMs)}
      </span>
      <span
        className={`text-sm font-semibold w-12 ${
          source === "system" ? "text-primary" : "text-indigo"
        }`}
      >
        {source === "system" ? "Them" : "You"}
      </span>
      <p
        className={`font-serif text-base leading-relaxed text-foreground m-0 ${
          provisional ? "italic" : ""
        }`}
      >
        {text}
      </p>
    </div>
  );
}
