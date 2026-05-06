import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { LicenseStatus } from "./types";

const POLL_INTERVAL_MS = 60_000;

export function useLicenseStatus() {
  const [status, setStatus] = useState<LicenseStatus | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const next = await invoke<LicenseStatus>("get_license_status");
      setStatus(next);
    } catch {
      // Treat read failures as no-op; status stays at last known value.
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    refresh();
    const id = window.setInterval(() => {
      if (cancelled) return;
      void refresh();
    }, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [refresh]);

  return { status, loading, refresh };
}
