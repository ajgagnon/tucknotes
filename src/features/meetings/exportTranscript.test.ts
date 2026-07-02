import { describe, it, expect } from "vitest";
import {
  buildExportContent,
  exportFilenameBase,
  formatTranscript,
  type TranscriptLine,
} from "./exportTranscript";
import type { MeetingRow } from "./types";

const lines: TranscriptLine[] = [
  { timestamp_ms: 0, source: "local", text: "Hello there" },
  { timestamp_ms: 65_000, source: "system", text: "Hi, good to meet you" },
];

describe("formatTranscript", () => {
  it("plain text: no timestamps, no speakers", () => {
    expect(
      formatTranscript(lines, { timestamps: false, speakers: false }),
    ).toBe("Hello there\nHi, good to meet you");
  });

  it("with speakers only (You/Them)", () => {
    expect(formatTranscript(lines, { timestamps: false, speakers: true })).toBe(
      "You: Hello there\nThem: Hi, good to meet you",
    );
  });

  it("with timestamps only", () => {
    expect(formatTranscript(lines, { timestamps: true, speakers: false })).toBe(
      "[00:00:00] Hello there\n[00:01:05] Hi, good to meet you",
    );
  });

  it("with timestamps and speakers", () => {
    expect(formatTranscript(lines, { timestamps: true, speakers: true })).toBe(
      "[00:00:00] You: Hello there\n[00:01:05] Them: Hi, good to meet you",
    );
  });

  it("returns empty string for no segments", () => {
    expect(formatTranscript([], { timestamps: true, speakers: true })).toBe("");
  });
});

const meeting: MeetingRow = {
  id: "m1",
  title: "Weekly Sync",
  // 2024-01-15T09:30:00 local
  created_at: new Date(2024, 0, 15, 9, 30, 0).getTime(),
  ended_at: null,
  duration_ms: null,
  template: null,
};

describe("buildExportContent", () => {
  it("prepends a title/date header and the full transcript", () => {
    const out = buildExportContent(meeting, lines);
    expect(out.startsWith("Weekly Sync\n")).toBe(true);
    expect(out).toContain("[00:00:00] You: Hello there");
    expect(out).toContain("[00:01:05] Them: Hi, good to meet you");
    expect(out.endsWith("\n")).toBe(true);
  });

  it("falls back to a placeholder title", () => {
    const out = buildExportContent({ ...meeting, title: null }, lines);
    expect(out.startsWith("Untitled meeting\n")).toBe(true);
  });
});

describe("exportFilenameBase", () => {
  it("combines a sanitized title with the date", () => {
    expect(exportFilenameBase(meeting)).toBe("Weekly Sync 2024-01-15");
  });

  it("strips filesystem-unsafe characters", () => {
    expect(exportFilenameBase({ ...meeting, title: "Q1/Review: Plan?" })).toBe(
      "Q1Review Plan 2024-01-15",
    );
  });

  it("falls back when title is empty", () => {
    expect(exportFilenameBase({ ...meeting, title: "   " })).toBe(
      "transcript 2024-01-15",
    );
  });
});
