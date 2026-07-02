import type { UnlistenFn } from "@tauri-apps/api/event";

/**
 * Combine several pending listen() registrations into a single unlisten.
 * For imperative, dynamically-scoped listener groups (e.g. per-request
 * streams). Component-lifetime subscriptions should use useTauriEvent.
 *
 * If any registration fails, the ones that succeeded are unlistened
 * before the error is rethrown, so no listener leaks.
 */
export async function listenBatch(
  registrations: ReadonlyArray<Promise<UnlistenFn>>,
): Promise<UnlistenFn> {
  const results = await Promise.allSettled(registrations);
  const fns = results
    .filter(
      (r): r is PromiseFulfilledResult<UnlistenFn> => r.status === "fulfilled",
    )
    .map((r) => r.value);
  const failed = results.find(
    (r): r is PromiseRejectedResult => r.status === "rejected",
  );
  if (failed) {
    for (const fn of fns) fn();
    throw failed.reason;
  }
  return () => {
    for (const fn of fns) fn();
  };
}
