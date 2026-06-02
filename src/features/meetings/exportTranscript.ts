import { formatTranscriptTimestamp } from "@/lib/format-time";
import { formatClockTime, formatWeekdayMonthDayOrdinal } from "@/lib/format-date";
import type { MeetingRow } from "./types";

/** Minimal shape shared by persisted (`SegmentRow`) and live (`TranscriptSegment`) segments. */
export interface TranscriptLine {
  timestamp_ms: number;
  source: string;
  text: string;
}

export interface FormatOpts {
  timestamps: boolean;
  speakers: boolean;
}

/** Speaker label matching the on-screen convention (see `SegmentBubble`). */
function speakerLabel(source: string): string {
  return source === "system" ? "Them" : "You";
}

/** Format transcript segments into plain text, one segment per line. */
export function formatTranscript(
  lines: TranscriptLine[],
  opts: FormatOpts,
): string {
  return lines
    .map((line) => {
      const parts: string[] = [];
      if (opts.timestamps) {
        parts.push(`[${formatTranscriptTimestamp(line.timestamp_ms)}]`);
      }
      if (opts.speakers) {
        parts.push(`${speakerLabel(line.source)}:`);
      }
      parts.push(line.text);
      return parts.join(" ");
    })
    .join("\n");
}

/** Full export content: a meeting header followed by the complete transcript. */
export function buildExportContent(
  meeting: MeetingRow,
  lines: TranscriptLine[],
): string {
  const title = meeting.title?.trim() || "Untitled meeting";
  const header = `${title}\n${formatWeekdayMonthDayOrdinal(meeting.created_at)} · ${formatClockTime(meeting.created_at)}`;
  const body = formatTranscript(lines, { timestamps: true, speakers: true });
  return `${header}\n\n${body}\n`;
}

/** A filesystem-safe default filename (without extension) for a meeting export. */
export function exportFilenameBase(meeting: MeetingRow): string {
  const title = meeting.title?.trim() || "transcript";
  const date = new Date(meeting.created_at);
  const stamp = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
  const safeTitle = title.replace(/[^\p{L}\p{N} _-]/gu, "").trim() || "transcript";
  return `${safeTitle} ${stamp}`;
}
