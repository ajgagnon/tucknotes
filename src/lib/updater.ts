import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { message } from "@tauri-apps/plugin-dialog";
import { toastError } from "@/lib/toast";

export type UpdaterState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "up-to-date" }
  | { kind: "available"; update: Update }
  | { kind: "downloading" }
  | { kind: "ready" }
  | { kind: "error"; message: string };

type Listener = (state: UpdaterState) => void;

let state: UpdaterState = { kind: "idle" };
let dialogOpen = false;
let checkPromise: Promise<UpdaterState> | null = null;
let installPromise: Promise<UpdaterState> | null = null;
const listeners = new Set<Listener>();

function setState(next: UpdaterState) {
  state = next;
  for (const listener of listeners) listener(state);
}

export function getState(): UpdaterState {
  return state;
}

export function subscribe(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function isDialogOpen(): boolean {
  return dialogOpen;
}

export function setDialogOpen(value: boolean): void {
  dialogOpen = value;
}

export function checkOnce(): Promise<UpdaterState> {
  if (checkPromise) return checkPromise;
  // Don't clobber an in-flight install.
  if (state.kind === "downloading" || state.kind === "ready") {
    return Promise.resolve(state);
  }
  setState({ kind: "checking" });
  checkPromise = (async () => {
    try {
      const update = await check();
      if (update) {
        setState({ kind: "available", update });
      } else {
        setState({ kind: "up-to-date" });
      }
    } catch (e: unknown) {
      setState({ kind: "error", message: String(e) });
      toastError(`Update check failed: ${String(e)}`);
    } finally {
      checkPromise = null;
    }
    return state;
  })();
  return checkPromise;
}

export function installOnce(): Promise<UpdaterState> {
  if (installPromise) return installPromise;
  if (state.kind !== "available") return Promise.resolve(state);
  const update = state.update;
  setState({ kind: "downloading" });
  installPromise = (async () => {
    try {
      await update.downloadAndInstall();
      setState({ kind: "ready" });
    } catch (e: unknown) {
      setState({ kind: "error", message: String(e) });
      toastError(`Update failed: ${String(e)}`);
    } finally {
      installPromise = null;
    }
    return state;
  })();
  return installPromise;
}

export async function relaunchOnce(): Promise<void> {
  if (state.kind !== "ready") return;
  try {
    await relaunch();
  } catch (e: unknown) {
    setState({ kind: "error", message: String(e) });
    try {
      await message(
        "Restart failed. Please quit and reopen TuckNotes manually to finish updating.",
        { title: "Restart failed", kind: "error" },
      );
    } catch {
      // Nothing else to do.
    }
  }
}

export function resetIfTerminal(): void {
  if (state.kind === "up-to-date" || state.kind === "error") {
    setState({ kind: "idle" });
  }
}
