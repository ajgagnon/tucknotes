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
export interface TitlePayload {
  meeting_id: string;
  title: string;
}

/** One section in the `summary:plan` event, in document order. */
export interface PlanSection {
  index: number;
  heading: string;
}

/** `summary:plan` — the ordered sections the run will write, emitted up front. */
export interface SummaryPlanPayload {
  meeting_id: string;
  sections: PlanSection[];
}

/** `summary:section_start` — a section's pass has begun (UI shows "thinking"). */
export interface SectionStartPayload {
  meeting_id: string;
  index: number;
}

/** `summary:token` — a body token for section `index` (heading rendered by UI). */
export interface SectionTokenPayload {
  meeting_id: string;
  index: number;
  token: string;
}

/** `summary:section_done` — a section's pass finished; `empty` sections collapse. */
export interface SectionDonePayload {
  meeting_id: string;
  index: number;
  empty: boolean;
}

export type SummarySectionState =
  "pending" | "thinking" | "writing" | "done" | "skipped";

/** View-model for one streaming summary section, derived from the events. */
export interface SummarySection {
  heading: string;
  body: string;
  state: SummarySectionState;
}

export interface SummarizationQueue {
  active: string | null;
  pending: string[];
}
