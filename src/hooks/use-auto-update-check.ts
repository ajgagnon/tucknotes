import { useEffect } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  checkOnce,
  installOnce,
  relaunchOnce,
  isDialogOpen,
  setDialogOpen,
} from "@/lib/updater";

const STARTUP_DELAY_MS = 5_000;
const INTERVAL_MS = 4 * 60 * 60 * 1000;
const FOCUS_MIN_GAP_MS = 60 * 60 * 1000;

export function useAutoUpdateCheck({ enabled }: { enabled: boolean }) {
  useEffect(() => {
    if (!enabled) return;

    let cancelled = false;
    let lastTickAt = 0;

    async function runAvailableFlow(version: string) {
      if (cancelled || isDialogOpen()) return;
      setDialogOpen(true);
      let install: boolean;
      try {
        install = await ask(
          `A new version (v${version}) is available. Install now?`,
          {
            title: "Update available",
            okLabel: "Install",
            cancelLabel: "Later",
          },
        );
      } catch (e) {
        console.warn("auto-update: prompt failed", e);
        return;
      } finally {
        setDialogOpen(false);
      }
      if (!install || cancelled) return;

      const result = await installOnce();
      if (cancelled || result.kind !== "ready") return;
      await runReadyFlow();
    }

    async function runReadyFlow() {
      if (cancelled || isDialogOpen()) return;
      setDialogOpen(true);
      let restart: boolean;
      try {
        restart = await ask("The update is ready. Restart TuckNotes now?", {
          title: "Restart to apply update",
          okLabel: "Restart",
          cancelLabel: "Later",
        });
      } catch (e) {
        console.warn("auto-update: prompt failed", e);
        return;
      } finally {
        setDialogOpen(false);
      }
      if (!restart || cancelled) return;
      await relaunchOnce();
    }

    async function tick(reason: "startup" | "interval" | "focus") {
      const now = Date.now();
      if (reason === "focus" && now - lastTickAt < FOCUS_MIN_GAP_MS) return;
      lastTickAt = now;

      const result = await checkOnce();
      if (cancelled) return;
      if (result.kind === "available") {
        await runAvailableFlow(result.update.version);
      } else if (result.kind === "ready") {
        await runReadyFlow();
      }
    }

    function fire(reason: "startup" | "interval" | "focus") {
      tick(reason).catch((e) => console.warn("auto-update: tick failed", e));
    }

    const startupTimer = window.setTimeout(
      () => fire("startup"),
      STARTUP_DELAY_MS,
    );
    const intervalId = window.setInterval(
      () => fire("interval"),
      INTERVAL_MS,
    );
    const onFocus = () => fire("focus");
    window.addEventListener("focus", onFocus);

    return () => {
      cancelled = true;
      window.clearTimeout(startupTimer);
      window.clearInterval(intervalId);
      window.removeEventListener("focus", onFocus);
    };
  }, [enabled]);
}
