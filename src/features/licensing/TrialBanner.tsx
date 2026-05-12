import { AlertTriangle, Clock } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useLicenseStatus } from "./use-license-status";
import { BUY_URL } from "./types";

interface TrialBannerProps {
  onOpenSettings: () => void;
}

export function TrialBanner({ onOpenSettings }: TrialBannerProps) {
  const { status } = useLicenseStatus();
  if (!status) return null;
  if (status.kind === "Licensed") return null;

  const expired =
    status.kind === "TrialExpired" || status.kind === "LicenseInvalid";

  if (expired) {
    return (
      <button
        type="button"
        onClick={onOpenSettings}
        className="mb-2 w-full rounded-md border border-destructive/30 bg-destructive/10 px-2.5 py-2 text-left text-xs text-destructive hover:bg-destructive/15 transition-colors flex items-center gap-2"
      >
        <AlertTriangle className="size-3.5 shrink-0" />
        <span className="flex-1">
          {status.kind === "TrialExpired" ? "Trial expired" : "License invalid"}{" "}
          · Enter key
        </span>
      </button>
    );
  }

  // Trial — only show banner in the last 7 days to avoid noise on day 1.
  if (status.days_remaining > 7) return null;

  return (
    <button
      type="button"
      onClick={() => openUrl(BUY_URL).catch(() => onOpenSettings())}
      className="mb-2 w-full rounded-md border border-border bg-muted/50 px-2.5 py-2 text-left text-xs text-muted-foreground hover:bg-muted transition-colors flex items-center gap-2"
    >
      <Clock className="size-3.5 shrink-0" />
      <span className="flex-1">
        {status.days_remaining} {status.days_remaining === 1 ? "day" : "days"}{" "}
        left in trial
      </span>
      <span className="text-primary font-medium">Buy</span>
    </button>
  );
}
