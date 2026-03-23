export interface MeetingRow {
  id: string;
  title: string | null;
  created_at: number;
  ended_at: number | null;
  duration_ms: number | null;
  summary: string | null;
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
