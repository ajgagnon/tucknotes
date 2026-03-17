import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

function MeetingOverlay() {
  const [appName, setAppName] = useState<string>("Meeting");
  const [starting, setStarting] = useState(false);

  useEffect(() => {
    // Read the app name from the URL query parameter
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
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: 8,
        boxSizing: "border-box",
        fontFamily:
          '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
        WebkitUserSelect: "none",
        cursor: "default",
      }}
      data-tauri-drag-region
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          background: "rgba(30, 30, 30, 0.92)",
          backdropFilter: "blur(20px)",
          WebkitBackdropFilter: "blur(20px)",
          borderRadius: 14,
          padding: "10px 16px",
          width: "100%",
          boxShadow: "0 4px 24px rgba(0,0,0,0.3), 0 0 0 0.5px rgba(255,255,255,0.1)",
        }}
      >
        {/* Pulsing red dot */}
        <div
          style={{
            width: 10,
            height: 10,
            borderRadius: "50%",
            background: "#ef4444",
            flexShrink: 0,
            animation: "pulse 2s ease-in-out infinite",
          }}
        />

        {/* App name */}
        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            style={{
              color: "rgba(255,255,255,0.6)",
              fontSize: 11,
              lineHeight: 1,
              marginBottom: 2,
            }}
          >
            Meeting detected
          </div>
          <div
            style={{
              color: "#fff",
              fontSize: 13,
              fontWeight: 600,
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            {appName}
          </div>
        </div>

        {/* Record button */}
        <button
          onClick={handleStart}
          disabled={starting}
          style={{
            background: "#ef4444",
            color: "#fff",
            border: "none",
            borderRadius: 8,
            padding: "6px 14px",
            fontSize: 12,
            fontWeight: 600,
            cursor: starting ? "default" : "pointer",
            opacity: starting ? 0.6 : 1,
            whiteSpace: "nowrap",
            transition: "opacity 0.15s, background 0.15s",
            flexShrink: 0,
          }}
          onMouseEnter={(e) => {
            if (!starting) e.currentTarget.style.background = "#dc2626";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = "#ef4444";
          }}
        >
          {starting ? "Starting..." : "Record"}
        </button>

        {/* Dismiss button */}
        <button
          onClick={handleDismiss}
          style={{
            background: "transparent",
            border: "none",
            color: "rgba(255,255,255,0.4)",
            fontSize: 16,
            cursor: "pointer",
            padding: "2px 4px",
            lineHeight: 1,
            flexShrink: 0,
            transition: "color 0.15s",
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.color = "rgba(255,255,255,0.8)";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.color = "rgba(255,255,255,0.4)";
          }}
          title="Dismiss"
        >
          &times;
        </button>
      </div>

      <style>{`
        @keyframes pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.4; }
        }
      `}</style>
    </div>
  );
}

export default MeetingOverlay;
