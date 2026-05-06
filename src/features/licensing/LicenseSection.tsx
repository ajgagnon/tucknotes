import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CheckCircle2, AlertTriangle } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useLicenseStatus } from "./use-license-status";
import { BUY_URL, type LicenseStatus } from "./types";

function formatDate(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

function StatusBadge({ status }: { status: LicenseStatus }) {
  switch (status.kind) {
    case "Trial":
      return (
        <p className="text-sm text-muted-foreground">
          Trial · {status.days_remaining}{" "}
          {status.days_remaining === 1 ? "day" : "days"} remaining
        </p>
      );
    case "TrialExpired":
      return <p className="text-sm text-destructive">Trial expired</p>;
    case "Licensed":
      return (
        <p className="text-sm text-muted-foreground inline-flex items-center gap-1">
          <CheckCircle2 className="size-4 text-success" />
          Licensed · last verified {formatDate(status.last_validated_at)}
        </p>
      );
    case "LicenseInvalid":
      return <p className="text-sm text-destructive">License invalid</p>;
  }
}

export function LicenseSection() {
  const { status, refresh } = useLicenseStatus();
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleActivate() {
    if (busy) return;
    setError(null);
    setBusy(true);
    try {
      await invoke<LicenseStatus>("activate_license_key", { key });
      setKey("");
      await refresh();
    } catch (e: unknown) {
      const err = e as { kind?: string; message?: string };
      setError(err.message || "Activation failed.");
    } finally {
      setBusy(false);
    }
  }

  async function handleDeactivate() {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await invoke<LicenseStatus>("deactivate_license");
      await refresh();
    } catch (e: unknown) {
      const err = e as { message?: string };
      setError(err.message || "Could not deactivate license.");
    } finally {
      setBusy(false);
    }
  }

  async function handleBuy() {
    try {
      await openUrl(BUY_URL);
    } catch {
      // Opener errors are non-actionable for the user.
    }
  }

  const isLicensed = status?.kind === "Licensed";
  const isInvalid = status?.kind === "LicenseInvalid";
  const isExpired = status?.kind === "TrialExpired" || isInvalid;

  return (
    <section>
      <h2 className="text-sm font-medium text-muted-foreground mb-4">
        License
      </h2>
      <Card>
        <CardContent>
          {status && (
            <div className="mb-3">
              <StatusBadge status={status} />
            </div>
          )}

          {isExpired && (
            <Alert variant="destructive" className="mb-4">
              <AlertTriangle />
              <AlertTitle>
                {isInvalid ? "License invalid" : "Trial expired"}
              </AlertTitle>
              <AlertDescription>
                {isInvalid
                  ? (status as { kind: "LicenseInvalid"; reason: string })
                      .reason
                  : "Recording and summarization are disabled. Enter a license key to continue using TuckNotes."}
              </AlertDescription>
            </Alert>
          )}

          {isLicensed ? (
            <div className="flex items-center justify-between gap-3">
              <Button
                variant="outline"
                size="sm"
                onClick={handleDeactivate}
                disabled={busy}
              >
                Deactivate this device
              </Button>
            </div>
          ) : (
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <Input
                  placeholder="Paste license key…"
                  value={key}
                  onChange={(e) => setKey(e.target.value)}
                  disabled={busy}
                  autoComplete="off"
                  spellCheck={false}
                  className="font-mono"
                />
                <Button onClick={handleActivate} disabled={busy || !key.trim()}>
                  {busy ? "Activating…" : "Activate"}
                </Button>
              </div>
              {error && <p className="text-xs text-destructive">{error}</p>}
              <p className="text-xs text-muted-foreground">
                Don't have a key yet?{" "}
                <button
                  type="button"
                  onClick={handleBuy}
                  className="text-primary underline-offset-4 hover:underline"
                >
                  Buy TuckNotes
                </button>
              </p>
            </div>
          )}
        </CardContent>
      </Card>
    </section>
  );
}
