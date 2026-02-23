import { describe, it, expect } from "vitest";
import { toAppError } from "../hooks/useRecording";

describe("toAppError", () => {
  it("passes through a valid AppError object", () => {
    const err = { kind: "PermissionDenied", message: "Not allowed" };
    expect(toAppError(err)).toEqual(err);
  });

  it("wraps a plain string into an Unknown error", () => {
    expect(toAppError("something broke")).toEqual({
      kind: "Unknown",
      message: "something broke",
    });
  });

  it("wraps a plain Error into an Unknown error", () => {
    const result = toAppError(new Error("oops"));
    expect(result.kind).toBe("Unknown");
    expect(result.message).toContain("oops");
  });

  it("wraps null into an Unknown error", () => {
    expect(toAppError(null)).toEqual({
      kind: "Unknown",
      message: "null",
    });
  });

  it("wraps undefined into an Unknown error", () => {
    expect(toAppError(undefined)).toEqual({
      kind: "Unknown",
      message: "undefined",
    });
  });
});
