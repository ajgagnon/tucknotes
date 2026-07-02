import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";

/** Recording-time behavior. `disabled` while a recording is active — the
 *  live-minutes setting is read once when a session starts, so mid-recording
 *  changes wouldn't apply anyway. */
export function RecordingSection({ disabled }: { disabled?: boolean }) {
  const [liveMinutes, setLiveMinutes] = useState<boolean | null>(null);

  useEffect(() => {
    invoke<boolean>("get_live_minutes_enabled")
      .then(setLiveMinutes)
      .catch((e) => console.error("get_live_minutes_enabled:", e));
  }, []);

  const handleToggle = (checked: boolean) => {
    setLiveMinutes(checked);
    invoke("set_live_minutes_enabled", { enabled: checked }).catch((e) => {
      console.error("set_live_minutes_enabled:", e);
      setLiveMinutes(!checked);
    });
  };

  return (
    <section>
      <h2 className="text-sm font-medium text-muted-foreground mb-4">
        Recording
      </h2>
      <div className="flex items-start justify-between gap-4 rounded-lg border p-4">
        <div className="grid gap-1">
          <Label htmlFor="live-minutes-switch">Live minutes</Label>
          <p className="text-xs text-muted-foreground">
            Keep a running bullet-point summary while you record, updated as the
            transcript comes in. Requires a downloaded summarization model.
            Applies from the next recording.
          </p>
        </div>
        <Switch
          id="live-minutes-switch"
          checked={liveMinutes ?? false}
          onCheckedChange={handleToggle}
          disabled={disabled || liveMinutes === null}
        />
      </div>
    </section>
  );
}
