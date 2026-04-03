import { describe, it, expect } from "vitest";
import { rmsToLevel, smoothLevel } from "./audio-level";

describe("rmsToLevel", () => {
  it("returns 0 for rms = 0 (silence)", () => {
    expect(rmsToLevel(0)).toBe(0);
  });

  it("returns 1 for rms = 1.0 (full scale)", () => {
    expect(rmsToLevel(1.0)).toBe(1);
  });

  it("returns 0 for rms at -60dB floor (0.001)", () => {
    expect(rmsToLevel(0.001)).toBeCloseTo(0, 1);
  });

  it("returns ~0.5 for rms around -30dB (0.0316)", () => {
    // 20*log10(0.0316) ≈ -30.01 → (−30 + 60) / 60 ≈ 0.50
    expect(rmsToLevel(0.0316)).toBeCloseTo(0.5, 1);
  });

  it("clamps negative dB values below floor to 0", () => {
    expect(rmsToLevel(0.0001)).toBe(0); // -80dB, well below -60 floor
  });

  it("clamps values above 1.0 rms to 1", () => {
    expect(rmsToLevel(2.0)).toBe(1);
  });
});

describe("smoothLevel", () => {
  it("jumps to new level when higher than previous (fast attack)", () => {
    expect(smoothLevel(0.2, 0.8)).toBe(0.8);
  });

  it("decays gradually when new level is lower (slow decay)", () => {
    const result = smoothLevel(0.8, 0.1);
    // 0.8 * 0.92 + 0.1 * 0.08 = 0.736 + 0.008 = 0.744
    expect(result).toBeCloseTo(0.744, 3);
  });

  it("stays at same level when equal", () => {
    expect(smoothLevel(0.5, 0.5)).toBe(0.5);
  });

  it("decays toward zero from a peak", () => {
    let level = 1.0;
    for (let i = 0; i < 20; i++) {
      level = smoothLevel(level, 0);
    }
    expect(level).toBeLessThan(0.2);
  });
});
