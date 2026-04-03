import { describe, it, expect } from "vitest";
import { formatTime } from "./format-time";

describe("formatTime", () => {
  it("formats zero seconds", () => {
    expect(formatTime(0)).toBe("00:00");
  });

  it("formats seconds only", () => {
    expect(formatTime(5)).toBe("00:05");
    expect(formatTime(59)).toBe("00:59");
  });

  it("formats minutes and seconds", () => {
    expect(formatTime(60)).toBe("01:00");
    expect(formatTime(61)).toBe("01:01");
    expect(formatTime(125)).toBe("02:05");
  });

  it("formats hours, minutes, and seconds", () => {
    expect(formatTime(3600)).toBe("1:00:00");
    expect(formatTime(3661)).toBe("1:01:01");
    expect(formatTime(7384)).toBe("2:03:04");
  });
});
