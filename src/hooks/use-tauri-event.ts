import { useEffect, useRef } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface UseTauriEventOptions {
  /** When false, no subscription exists (default true). */
  enabled?: boolean;
}

/**
 * Subscribe to a Tauri event for the component's lifetime (or while
 * `enabled`). The handler lives in a ref, so callers can pass a fresh
 * closure every render without re-subscribing — no useCallback needed.
 */
export function useTauriEvent<T = unknown>(
  event: string,
  handler: (payload: T) => void,
  options?: UseTauriEventOptions,
): void {
  const enabled = options?.enabled ?? true;
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    if (!enabled) return;
    let active = true;
    let unlisten: UnlistenFn | null = null;
    void listen<T>(event, (e) => {
      // Drop deliveries that race the cleanup below (listen() resolves async).
      if (!active) return;
      handlerRef.current(e.payload);
    }).then((fn) => {
      // StrictMode can run cleanup before listen() resolves; unlisten now.
      if (active) unlisten = fn;
      else fn();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, [event, enabled]);
}
