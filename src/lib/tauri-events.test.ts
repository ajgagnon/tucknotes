import { describe, it, expect, vi } from "vitest";
import { listenBatch } from "./tauri-events";

describe("listenBatch", () => {
  it("resolves after all registrations and unlistens them all at once", async () => {
    const un1 = vi.fn();
    const un2 = vi.fn();
    const combined = await listenBatch([
      Promise.resolve(un1),
      Promise.resolve(un2),
    ]);

    expect(un1).not.toHaveBeenCalled();
    expect(un2).not.toHaveBeenCalled();

    combined();
    expect(un1).toHaveBeenCalledTimes(1);
    expect(un2).toHaveBeenCalledTimes(1);
  });

  it("unlistens successful registrations before rethrowing a failure", async () => {
    const un1 = vi.fn();
    await expect(
      listenBatch([
        Promise.resolve(un1),
        Promise.reject(new Error("registration failed")),
      ]),
    ).rejects.toThrow("registration failed");
    expect(un1).toHaveBeenCalledTimes(1);
  });

  it("returns a no-op unlisten for an empty batch", async () => {
    const combined = await listenBatch([]);
    expect(() => combined()).not.toThrow();
  });
});
