import { useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

const MODE_AUTOSTOP = "autostop";

function OverlayShell({ children }: { children: ReactNode }) {
  return (
    <div
      className="flex h-full w-full items-center justify-center p-2"
      style={{
        fontFamily:
          '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
      }}
      data-tauri-drag-region
    >
      <div className="flex w-full items-center gap-3 rounded-[14px] bg-[rgba(30,30,30,0.92)] px-4 py-2.5 shadow-[0_4px_24px_rgba(0,0,0,0.3)] backdrop-blur-[20px]">
        {children}
      </div>
    </div>
  );
}

function MeetingOverlay() {
  const params = new URLSearchParams(window.location.search);
  const mode = params.get("mode");
  const appName = params.get("app")
    ? decodeURIComponent(params.get("app")!)
    : null;

  if (mode === MODE_AUTOSTOP) {
    return (
      <OverlayShell>
        <AutoStopContent appName={appName} />
      </OverlayShell>
    );
  }

  return (
    <OverlayShell>
      <MeetingDetectedContent appName={appName ?? "Meeting"} />
    </OverlayShell>
  );
}

function MeetingDetectedContent({ appName }: { appName: string }) {
  const [starting, setStarting] = useState(false);

  const handleStart = async () => {
    setStarting(true);
    try {
      await invoke("start_recording");
      await getCurrentWindow().close();
    } catch (e) {
      console.error("Failed to start recording:", e);
      setStarting(false);
    }
  };

  const handleDismiss = () => {
    void getCurrentWindow().close();
  };

  return (
    <>
      <div className="size-2.5 shrink-0 animate-pulse rounded-full bg-red-500" />

      <div className="min-w-0 flex-1">
        <div className="text-[11px] leading-none text-white/60">
          Meeting detected
        </div>
        <div className="mt-0.5 truncate text-[13px] font-semibold text-white">
          {appName}
        </div>
      </div>

      <button
        onClick={handleStart}
        disabled={starting}
        className="shrink-0 rounded-lg bg-red-500 px-3.5 py-1.5 text-xs font-semibold text-white transition-[opacity,background-color] hover:bg-red-600 disabled:cursor-default disabled:opacity-60"
      >
        {starting ? "Starting..." : "Record"}
      </button>

      <button
        onClick={handleDismiss}
        className="shrink-0 px-1 py-0.5 text-base leading-none text-white/40 transition-colors hover:text-white/80"
        title="Dismiss"
      >
        &times;
      </button>
    </>
  );
}

function AutoStopContent({ appName }: { appName: string | null }) {
  const [stopping, setStopping] = useState(false);

  const handleStop = async () => {
    setStopping(true);
    try {
      await invoke("stop_recording");
    } catch (e) {
      console.error("stop_recording:", e);
    }
    void getCurrentWindow().close();
  };

  const handleKeep = () => {
    void invoke("request_auto_stop_cancel").catch((e) =>
      console.error("request_auto_stop_cancel:", e),
    );
    void getCurrentWindow().close();
  };

  return (
    <>
      <div className="size-2.5 shrink-0 animate-pulse rounded-full bg-amber-400" />

      <div className="min-w-0 flex-1">
        <div className="text-[11px] leading-none text-white/60">
          {appName ? `${appName} call ended` : "Meeting ended"}
        </div>
        <div className="mt-0.5 truncate text-[13px] font-semibold text-white">
          Still recording
        </div>
      </div>

      <button
        onClick={handleKeep}
        className="shrink-0 rounded-lg bg-white/15 px-3 py-1.5 text-xs font-semibold text-white transition-[opacity,background-color] hover:bg-white/25"
      >
        Continue
      </button>

      <button
        onClick={handleStop}
        disabled={stopping}
        className="shrink-0 rounded-lg bg-red-500 px-3 py-1.5 text-xs font-semibold text-white transition-[opacity,background-color] hover:bg-red-600 disabled:cursor-default disabled:opacity-60"
      >
        {stopping ? "Stopping…" : "Stop"}
      </button>
    </>
  );
}

export default MeetingOverlay;
