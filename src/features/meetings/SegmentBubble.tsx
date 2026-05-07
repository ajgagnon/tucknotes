export function SegmentBubble({
  source,
  text,
  provisional,
}: {
  source: string;
  text: string;
  provisional?: boolean;
}) {
  return (
    <div className={`flex flex-col gap-0.5 ${provisional ? "opacity-50" : ""}`}>
      <span
        className={`text-[0.65rem] font-semibold uppercase tracking-wider ${
          source === "system" ? "text-primary" : "text-indigo"
        }`}
      >
        {source === "system" ? "Them" : "You"}
      </span>
      <p
        className={`text-sm text-neutral-700 dark:text-neutral-300 m-0 leading-snug ${provisional ? "italic" : ""}`}
      >
        {text}
      </p>
    </div>
  );
}
