export interface TranscriptSegment {
  text: string;
  source: string;
  timestamp_ms: number;
  is_provisional: boolean;
}

export interface AppError {
  kind: string;
  message: string;
}

export function toAppError(e: unknown): AppError {
  if (typeof e === "object" && e !== null && "kind" in e && "message" in e) {
    return e as AppError;
  }
  return { kind: "Unknown", message: String(e) };
}
