import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  checkOnce,
  installOnce,
  relaunchOnce,
  getState,
  subscribe,
  resetIfTerminal,
  type UpdaterState,
} from "@/lib/updater";

export function UpdateSection() {
  const [version, setVersion] = useState<string | null>(null);
  const [state, setState] = useState<UpdaterState>(() => getState());

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => setVersion(null));
  }, []);

  useEffect(() => subscribe(setState), []);

  async function handleCheck() {
    resetIfTerminal();
    await checkOnce();
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
              <Button size="sm" onClick={() => relaunchOnce()}>
                Restart to apply update
              </Button>
            ) : state.kind === "available" ? (
              <Button size="sm" onClick={() => installOnce()}>
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
        </CardContent>
      </Card>
    </section>
  );
}
