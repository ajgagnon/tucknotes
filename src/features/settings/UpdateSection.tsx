import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";

type State =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "up-to-date" }
  | { kind: "available"; update: Update }
  | { kind: "downloading" }
  | { kind: "ready" }
  | { kind: "error"; message: string };

export function UpdateSection() {
  const [version, setVersion] = useState<string | null>(null);
  const [state, setState] = useState<State>({ kind: "idle" });

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => setVersion(null));
  }, []);

  async function handleCheck() {
    setState({ kind: "checking" });
    try {
      const update = await check();
      if (update) {
        setState({ kind: "available", update });
      } else {
        setState({ kind: "up-to-date" });
      }
    } catch (e: unknown) {
      setState({ kind: "error", message: String(e) });
    }
  }

  async function handleInstall() {
    if (state.kind !== "available") return;
    setState({ kind: "downloading" });
    try {
      await state.update.downloadAndInstall();
      setState({ kind: "ready" });
    } catch (e: unknown) {
      setState({ kind: "error", message: String(e) });
    }
  }

  async function handleRelaunch() {
    try {
      await relaunch();
    } catch (e: unknown) {
      setState({ kind: "error", message: String(e) });
    }
  }

  return (
    <section>
      <h2 className="text-sm font-medium text-muted-foreground mb-4">
        Updates
      </h2>
      <Card>
        <CardContent>
          <div className="flex items-center justify-between gap-3">
            <p className="text-sm text-muted-foreground">
              {version ? `TuckNotes v${version}` : "TuckNotes"}
            </p>
            {state.kind === "ready" ? (
              <Button size="sm" onClick={handleRelaunch}>
                Restart to apply update
              </Button>
            ) : state.kind === "available" ? (
              <Button size="sm" onClick={handleInstall}>
                Install v{state.update.version}
              </Button>
            ) : (
              <Button
                size="sm"
                variant="outline"
                onClick={handleCheck}
                disabled={
                  state.kind === "checking" || state.kind === "downloading"
                }
              >
                {state.kind === "checking"
                  ? "Checking…"
                  : state.kind === "downloading"
                    ? "Downloading…"
                    : "Check for updates"}
              </Button>
            )}
          </div>
          {state.kind === "up-to-date" && (
            <p className="mt-2 text-xs text-muted-foreground">
              You're on the latest version.
            </p>
          )}
          {state.kind === "error" && (
            <p className="mt-2 text-xs text-destructive">{state.message}</p>
          )}
        </CardContent>
      </Card>
    </section>
  );
}
