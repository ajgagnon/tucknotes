import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

function MeetingOverlay() {
  const [appName, setAppName] = useState<string>("Meeting");
  const [starting, setStarting] = useState(false);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const name = params.get("app");
    if (name) setAppName(decodeURIComponent(name));
  }, []);

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

  const handleDismiss = async () => {
    await getCurrentWindow().close();
  };

  return (
    <div
      className="flex h-full w-full items-center justify-center p-2"
      style={{ fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif' }}
      data-tauri-drag-region
    >
      <div className="flex w-full items-center gap-3 rounded-[14px] bg-[rgba(30,30,30,0.92)] px-4 py-2.5 shadow-[0_4px_24px_rgba(0,0,0,0.3)] backdrop-blur-[20px]">
        {/* Pulsing red dot */}
        <div className="size-2.5 shrink-0 animate-pulse rounded-full bg-red-500" />

        {/* App name */}
        <div className="min-w-0 flex-1">
          <div className="text-[11px] leading-none text-white/60">
            Meeting detected
          </div>
          <div className="mt-0.5 truncate text-[13px] font-semibold text-white">
            {appName}
          </div>
        </div>

        {/* Record button */}
        <button
          onClick={handleStart}
          disabled={starting}
          className="shrink-0 rounded-lg bg-red-500 px-3.5 py-1.5 text-xs font-semibold text-white transition-[opacity,background-color] hover:bg-red-600 disabled:cursor-default disabled:opacity-60"
        >
          {starting ? "Starting..." : "Record"}
        </button>

        {/* Dismiss button */}
        <button
          onClick={handleDismiss}
          className="shrink-0 px-1 py-0.5 text-base leading-none text-white/40 transition-colors hover:text-white/80"
          title="Dismiss"
        >
          &times;
        </button>
      </div>
    </div>
  );
}

export default MeetingOverlay;
