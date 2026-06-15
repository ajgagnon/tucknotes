export interface MeetingRow {
  id: string;
  title: string | null;
  created_at: number;
  ended_at: number | null;
  duration_ms: number | null;
  /** Summary template id used for this meeting (null = the Default template). */
  template: string | null;
}

/** A summary template summary, as returned by `list_summary_templates`. */
export interface TemplateInfo {
  id: string;
  name: string;
  description: string;
  /** `true` for templates that ship with the app (resettable, not deletable). */
  builtin: boolean;
}

export interface MeetingDocument {
  id: string;
  meeting_id: string;
  kind: "summary" | "notes" | "minutes";
  title: string;
  body: string | null;
  sort_order: number;
  created_at: number;
}

export function summaryBodyFromDocuments(
  documents: MeetingDocument[],
): string | null {
  return documents.find((d) => d.kind === "summary")?.body ?? null;
}

export interface SegmentRow {
  id: number;
  meeting_id: string;
  text: string;
  source: string;
  timestamp_ms: number;
  prompt: string | null;
  created_at: number;
}

export interface MeetingDetail {
  meeting: MeetingRow;
  segments: SegmentRow[];
  documents: MeetingDocument[];
}

/** Imperative scroll to a point in the transcript (live or persisted). */
export interface TranscriptScrollHandle {
  scrollToTimeMs(ms: number): void;
}

export interface MeetingTitleInfo {
  title: string | null;
  generatingTitle: boolean;
  createdAt: number;
  durationMs: number | null;
}

/** Event payload types matching the Rust structs */
export interface TokenPayload {
  meeting_id: string;
  token: string;
}

export interface TitlePayload {
  meeting_id: string;
  title: string;
}

export interface SummarizationQueue {
  active: string | null;
  pending: string[];
}
