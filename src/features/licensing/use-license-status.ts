import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { LicenseStatus } from "./types";

// Backstop poll — status is time-derived (trial/grace countdowns), so it
// must advance even without backend events.
const POLL_INTERVAL_MS = 60_000;

// Module-level store shared by every consumer (header gate, trial banner,
// settings) so an activation in one place updates all of them instantly.
let current: LicenseStatus | null = null;
let loaded = false;
const subscribers = new Set<() => void>();

function notify() {
  for (const cb of subscribers) cb();
}

export function getLicenseStatus(): LicenseStatus | null {
  return current;
}

export async function refreshLicenseStatus(): Promise<void> {
  try {
    current = await invoke<LicenseStatus>("get_license_status");
  } catch {
    // Treat read failures as no-op; status stays at last known value.
  }
  loaded = true;
  notify();
}

let started = false;
function ensureStarted() {
  if (started) return;
  started = true;
  void refreshLicenseStatus();
  setInterval(() => void refreshLicenseStatus(), POLL_INTERVAL_MS);
  // Backend emits on activate/deactivate/background revalidation.
  void listen<LicenseStatus>("license-status-changed", (event) => {
    current = event.payload;
    loaded = true;
    notify();
  });
}

export function subscribeLicenseStatus(cb: () => void): () => void {
  ensureStarted();
  subscribers.add(cb);
  return () => {
    subscribers.delete(cb);
  };
}

export function useLicenseStatus() {
  const status = useSyncExternalStore(
    subscribeLicenseStatus,
    getLicenseStatus,
    getLicenseStatus,
  );
  return { status, loading: !loaded, refresh: refreshLicenseStatus };
}
