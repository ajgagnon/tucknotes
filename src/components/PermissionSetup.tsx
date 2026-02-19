import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./PermissionSetup.css";

interface PermissionSetupProps {
  onComplete: () => void;
}

type MicStatus = "not_determined" | "authorized" | "denied" | "restricted" | "unknown";

function PermissionSetup({ onComplete }: PermissionSetupProps) {
  const [screenGranted, setScreenGranted] = useState(false);
  const [micStatus, setMicStatus] = useState<MicStatus>("not_determined");
  const [screenRequested, setScreenRequested] = useState(false);
  const [micRequesting, setMicRequesting] = useState(false);
  const [loading, setLoading] = useState(true);

  const checkPermissions = useCallback(async () => {
    try {
      const [screen, mic] = await Promise.all([
        invoke<boolean>("check_screen_recording_permission"),
        invoke<string>("check_microphone_permission"),
      ]);
      setScreenGranted(screen);
      setMicStatus(mic as MicStatus);
      setLoading(false);
    } catch {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    checkPermissions();
    const interval = setInterval(checkPermissions, 1500);
    return () => clearInterval(interval);
  }, [checkPermissions]);

  const handleScreenEnable = async () => {
    if (!screenRequested) {
      const result = await invoke<boolean>("request_screen_recording_permission");
      setScreenRequested(true);
      if (result) setScreenGranted(true);
    } else {
      await invoke("open_screen_recording_settings");
    }
  };

  const handleMicEnable = async () => {
    if (micStatus === "not_determined") {
      setMicRequesting(true);
      const granted = await invoke<boolean>("request_microphone_permission");
      setMicRequesting(false);
      setMicStatus(granted ? "authorized" : "denied");
    } else {
      await invoke("open_microphone_settings");
    }
  };

  if (loading) return null;

  const allGranted = screenGranted && micStatus === "authorized";

  if (allGranted) {
    return (
      <div className="permission-screen">
        <div className="permission-card">
          <div className="permission-icon success-icon">✓</div>
          <h1>You're all set!</h1>
          <p className="permission-subtitle">
            All permissions are granted. Grain can capture meeting audio.
          </p>
          <button className="permission-btn primary" onClick={onComplete}>
            Get Started
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="permission-screen">
      <div className="permission-card">
        <div className="permission-icon">🎙️</div>
        <h1>Before we get started</h1>
        <p className="permission-subtitle">
          Grain needs two macOS permissions to capture audio from your meetings.
        </p>

        {/* Screen Recording */}
        <div className={`permission-explainer ${screenGranted ? "granted" : ""}`}>
          <div className="explainer-header">
            <span className="explainer-icon">🖥️</span>
            <div>
              <h3>Screen Recording</h3>
              <span className={`status-badge ${screenGranted ? "granted" : "not-granted"}`}>
                {screenGranted ? "Granted" : "Not Granted"}
              </span>
            </div>
          </div>
          {!screenGranted && (
            <>
              <p>
                macOS requires the <strong>Screen Recording</strong> permission
                to capture system audio. There is no separate "audio only"
                permission — no video or screenshots are ever taken.
              </p>
              <button className="permission-btn secondary" onClick={handleScreenEnable}>
                {screenRequested ? "Open System Settings" : "Enable Screen Recording"}
              </button>
              {screenRequested && (
                <p className="permission-hint">
                  Find <strong>Grain</strong> in the list and toggle it on.
                </p>
              )}
            </>
          )}
        </div>

        {/* Microphone */}
        <div className={`permission-explainer ${micStatus === "authorized" ? "granted" : ""}`}>
          <div className="explainer-header">
            <span className="explainer-icon">🎤</span>
            <div>
              <h3>Microphone</h3>
              <span
                className={`status-badge ${micStatus === "authorized" ? "granted" : "not-granted"}`}
              >
                {micStatus === "authorized" ? "Granted" : "Not Granted"}
              </span>
            </div>
          </div>
          {micStatus !== "authorized" && (
            <>
              <p>
                Microphone access is needed to capture your voice during
                meetings. Audio is processed locally and never leaves your
                device.
              </p>
              <button
                className="permission-btn secondary"
                onClick={handleMicEnable}
                disabled={micRequesting}
              >
                {micRequesting
                  ? "Waiting for response…"
                  : micStatus === "denied" || micStatus === "restricted"
                    ? "Open System Settings"
                    : "Enable Microphone"}
              </button>
              {micStatus === "denied" && (
                <p className="permission-hint">
                  Toggle <strong>Grain</strong> on in System Settings &gt;
                  Privacy &amp; Security &gt; Microphone.
                </p>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

export default PermissionSetup;
