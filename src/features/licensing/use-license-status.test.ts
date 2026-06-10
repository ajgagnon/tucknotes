import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { LicenseStatus } from "./types";

const invokeMock = vi.fn();
let eventCallback: ((event: { payload: LicenseStatus }) => void) | null = null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (_name: string, cb: (event: { payload: LicenseStatus }) => void) => {
    eventCallback = cb;
    return Promise.resolve(() => {});
  },
}));

const TRIAL: LicenseStatus = { kind: "Trial", days_remaining: 5 };
const EXPIRED: LicenseStatus = { kind: "TrialExpired" };
const LICENSED: LicenseStatus = {
  kind: "Licensed",
  last_validated_at: 1_000,
  expires_grace_at: 2_000,
};

// The store is a module-level singleton, so each test gets a fresh copy via
// resetModules + dynamic import.
async function loadStore() {
  return await import("./use-license-status");
}

async function flushMicrotasks() {
  await Promise.resolve();
  await Promise.resolve();
}

beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers();
  invokeMock.mockReset();
  eventCallback = null;
});

afterEach(() => {
  vi.useRealTimers();
});

describe("license status store", () => {
  it("fetches the status when the first subscriber attaches", async () => {
    invokeMock.mockResolvedValue(TRIAL);
    const store = await loadStore();

    const cb = vi.fn();
    store.subscribeLicenseStatus(cb);
    await flushMicrotasks();

    expect(invokeMock).toHaveBeenCalledWith("get_license_status");
    expect(cb).toHaveBeenCalled();
    expect(store.getLicenseStatus()).toEqual(TRIAL);
  });

  it("notifies every subscriber on refresh, not just the caller's component", async () => {
    invokeMock.mockResolvedValue(EXPIRED);
    const store = await loadStore();

    const settings = vi.fn();
    const headerGate = vi.fn();
    const trialBanner = vi.fn();
    store.subscribeLicenseStatus(settings);
    store.subscribeLicenseStatus(headerGate);
    store.subscribeLicenseStatus(trialBanner);
    await flushMicrotasks();

    // The user activates a key in settings; refresh must update everyone.
    invokeMock.mockResolvedValue(LICENSED);
    settings.mockClear();
    headerGate.mockClear();
    trialBanner.mockClear();
    await store.refreshLicenseStatus();

    expect(store.getLicenseStatus()).toEqual(LICENSED);
    expect(settings).toHaveBeenCalled();
    expect(headerGate).toHaveBeenCalled();
    expect(trialBanner).toHaveBeenCalled();
  });

  it("applies backend license-status-changed events to all subscribers", async () => {
    invokeMock.mockResolvedValue(EXPIRED);
    const store = await loadStore();

    const cb = vi.fn();
    store.subscribeLicenseStatus(cb);
    await flushMicrotasks();
    expect(eventCallback).not.toBeNull();

    cb.mockClear();
    eventCallback!({ payload: LICENSED });

    expect(store.getLicenseStatus()).toEqual(LICENSED);
    expect(cb).toHaveBeenCalled();
  });

  it("polls as a backstop so time-derived status keeps advancing", async () => {
    invokeMock.mockResolvedValue(TRIAL);
    const store = await loadStore();

    store.subscribeLicenseStatus(vi.fn());
    await flushMicrotasks();
    expect(invokeMock).toHaveBeenCalledTimes(1);

    invokeMock.mockResolvedValue(EXPIRED);
    await vi.advanceTimersByTimeAsync(60_000);

    expect(invokeMock).toHaveBeenCalledTimes(2);
    expect(store.getLicenseStatus()).toEqual(EXPIRED);
  });

  it("keeps the last known status when a refresh fails", async () => {
    invokeMock.mockResolvedValue(LICENSED);
    const store = await loadStore();

    store.subscribeLicenseStatus(vi.fn());
    await flushMicrotasks();
    expect(store.getLicenseStatus()).toEqual(LICENSED);

    invokeMock.mockRejectedValue(new Error("ipc down"));
    await store.refreshLicenseStatus();

    expect(store.getLicenseStatus()).toEqual(LICENSED);
  });

  it("unsubscribing stops notifications for that subscriber only", async () => {
    invokeMock.mockResolvedValue(TRIAL);
    const store = await loadStore();

    const staying = vi.fn();
    const leaving = vi.fn();
    store.subscribeLicenseStatus(staying);
    const unsubscribe = store.subscribeLicenseStatus(leaving);
    await flushMicrotasks();

    unsubscribe();
    staying.mockClear();
    leaving.mockClear();
    await store.refreshLicenseStatus();

    expect(staying).toHaveBeenCalled();
    expect(leaving).not.toHaveBeenCalled();
  });
});
