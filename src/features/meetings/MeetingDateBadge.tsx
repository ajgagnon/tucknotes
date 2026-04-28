import { Calendar } from "lucide-react";

import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  formatClockTime,
  formatDurationShort,
  formatMonthDayOrdinal,
  formatWeekdayMonthDayOrdinal,
} from "@/lib/format-date";

export function MeetingDateBadge({
  title,
  createdAt,
  durationMs,
}: {
  title: string | null;
  createdAt: number;
  durationMs: number | null;
}) {
  return (
    <Popover>
      <PopoverTrigger
        className="inline-flex h-8 shrink-0 cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-full border border-muted px-3 text-xs font-semibold text-muted-foreground hover:text-foreground"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <Calendar
          className="size-3 shrink-0 text-muted-foreground"
          strokeWidth={2}
        />
        {formatMonthDayOrdinal(createdAt)}
      </PopoverTrigger>
      <PopoverContent className="min-w-[240px] p-3">
        <div className="text-sm font-semibold">{title || "Untitled"}</div>
        <div className="mt-0.5 text-xs text-muted-foreground">
          {formatWeekdayMonthDayOrdinal(createdAt)} ·{" "}
          {formatClockTime(createdAt)}
          {durationMs != null ? ` · ${formatDurationShort(durationMs)}` : ""}
        </div>
      </PopoverContent>
    </Popover>
  );
}
