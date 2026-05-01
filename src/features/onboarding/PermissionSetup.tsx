import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface PermissionSetupProps {
  onComplete: () => void;
}

type MicStatus = "not_determined" | "authorized" | "denied" | "restricted" | "unknown";

function PermissionSetup({ onComplete }: PermissionSetupProps) {
  const [screenGranted, setScreenGranted] = useState(false);
  const [micStatus, setMicStatus] = useState<MicStatus>("not_determined");
  const [accessibilityGranted, setAccessibilityGranted] = useState(false);
  const [screenRequested, setScreenRequested] = useState(false);
  const [micRequesting, setMicRequesting] = useState(false);
  const [accessibilityRequested, setAccessibilityRequested] = useState(false);
  const [loading, setLoading] = useState(true);

  const checkPermissions = useCallback(async () => {
    try {
      const [screen, mic, accessibility] = await Promise.all([
        invoke<boolean>("check_screen_recording_permission"),
        invoke<string>("check_microphone_permission"),
        invoke<boolean>("check_accessibility_permission"),
      ]);
      setScreenGranted(screen);
      setMicStatus(mic as MicStatus);
      setAccessibilityGranted(accessibility);
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

  const handleAccessibilityEnable = async () => {
    if (!accessibilityRequested) {
      const result = await invoke<boolean>("request_accessibility_permission");
      setAccessibilityRequested(true);
      if (result) setAccessibilityGranted(true);
    } else {
      await invoke("open_accessibility_settings");
    }
  };

  if (loading) return null;

  const allGranted =
    screenGranted && micStatus === "authorized" && accessibilityGranted;

  if (allGranted) {
    return (
      <div className="min-h-screen flex items-center justify-center p-8">
        <div className="max-w-[460px] w-full text-center">
          <div className="w-16 h-16 rounded-full bg-success text-white text-2xl flex items-center justify-center mx-auto mb-6">
            ✓
          </div>
          <h1 className="text-2xl font-bold mb-2">You're all set!</h1>
          <p className="text-neutral-500 dark:text-neutral-400 text-[0.95rem] mb-7 leading-relaxed">
            All permissions are granted. TuckNotes can capture meeting audio.
          </p>
          <button
            className="w-full border-none rounded-xl py-3 px-8 text-[0.95rem] font-semibold cursor-pointer bg-primary text-white shadow-[0_2px_8px_rgba(67,97,238,0.25)] transition-all duration-200 hover:bg-primary-hover hover:shadow-[0_4px_12px_rgba(67,97,238,0.35)] hover:-translate-y-px active:translate-y-0"
            onClick={onComplete}
          >
            Get Started
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen flex items-center justify-center p-8">
      <div className="max-w-[460px] w-full text-center">
        <div className="text-4xl mb-3 leading-none">🎙️</div>
        <h1 className="text-2xl font-bold mb-2">Before we get started</h1>
        <p className="text-neutral-500 dark:text-neutral-400 text-[0.95rem] mb-7 leading-relaxed">
          TuckNotes needs a few macOS permissions to capture audio and detect
          meetings automatically.
        </p>

        {/* Screen Recording */}
        <div
          className={`rounded-xl p-5 px-6 text-left mb-4 transition-all duration-300 ${
            screenGranted
              ? "border border-success/30 opacity-70 bg-black/3 dark:bg-white/5 dark:border-success/25"
              : "border border-black/8 bg-black/3 dark:bg-white/5 dark:border-white/10"
          }`}
        >
          <div className="flex items-start gap-3 mb-3">
            <span className="text-2xl leading-none shrink-0 mt-0.5">🖥️</span>
            <div>
              <h3 className="text-[0.95rem] font-semibold m-0 mb-1">Screen Recording</h3>
              <span
                className={`text-[0.7rem] font-semibold px-2 py-0.5 rounded-full inline-block tracking-tight ${
                  screenGranted
                    ? "bg-green-100 text-green-800 dark:bg-success/15 dark:text-success"
                    : "bg-amber-100 text-amber-800 dark:bg-amber-500/15 dark:text-amber-300"
                }`}
              >
                {screenGranted ? "Granted" : "Not Granted"}
              </span>
            </div>
          </div>
          {!screenGranted && (
            <>
              <p className="text-sm leading-relaxed text-neutral-600 dark:text-neutral-400 mb-3">
                macOS requires the <strong>Screen Recording</strong> permission
                to capture system audio. There is no separate "audio only"
                permission — no video or screenshots are ever taken.
              </p>
              <button
                className="border-[1.5px] border-primary dark:border-blue-400 text-primary dark:text-blue-400 bg-transparent rounded-xl py-2 px-6 text-sm font-semibold cursor-pointer transition-all duration-200 w-full mt-2 hover:bg-primary/8 dark:hover:bg-blue-400/10"
                onClick={handleScreenEnable}
              >
                {screenRequested ? "Open System Settings" : "Enable Screen Recording"}
              </button>
              {screenRequested && (
                <p className="text-xs text-neutral-400 dark:text-neutral-500 mt-4 leading-relaxed">
                  Find <strong>TuckNotes</strong> in the list and toggle it on.
                </p>
              )}
            </>
          )}
        </div>

        {/* Microphone */}
        <div
          className={`rounded-xl p-5 px-6 text-left mb-4 transition-all duration-300 ${
            micStatus === "authorized"
              ? "border border-success/30 opacity-70 bg-black/3 dark:bg-white/5 dark:border-success/25"
              : "border border-black/8 bg-black/3 dark:bg-white/5 dark:border-white/10"
          }`}
        >
          <div className="flex items-start gap-3 mb-3">
            <span className="text-2xl leading-none shrink-0 mt-0.5">🎤</span>
            <div>
              <h3 className="text-[0.95rem] font-semibold m-0 mb-1">Microphone</h3>
              <span
                className={`text-[0.7rem] font-semibold px-2 py-0.5 rounded-full inline-block tracking-tight ${
                  micStatus === "authorized"
                    ? "bg-green-100 text-green-800 dark:bg-success/15 dark:text-success"
                    : "bg-amber-100 text-amber-800 dark:bg-amber-500/15 dark:text-amber-300"
                }`}
              >
                {micStatus === "authorized" ? "Granted" : "Not Granted"}
              </span>
            </div>
          </div>
          {micStatus !== "authorized" && (
            <>
              <p className="text-sm leading-relaxed text-neutral-600 dark:text-neutral-400 mb-3">
                Microphone access is needed to capture your voice during
                meetings. Audio is processed locally and never leaves your
                device.
              </p>
              <button
                className="border-[1.5px] border-primary dark:border-blue-400 text-primary dark:text-blue-400 bg-transparent rounded-xl py-2 px-6 text-sm font-semibold cursor-pointer transition-all duration-200 w-full mt-2 hover:bg-primary/8 dark:hover:bg-blue-400/10 disabled:opacity-50 disabled:cursor-default"
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
                <p className="text-xs text-neutral-400 dark:text-neutral-500 mt-4 leading-relaxed">
                  Toggle <strong>TuckNotes</strong> on in System Settings &gt;
                  Privacy &amp; Security &gt; Microphone.
                </p>
              )}
            </>
          )}
        </div>

        {/* Accessibility */}
        <div
          className={`rounded-xl p-5 px-6 text-left mb-4 transition-all duration-300 ${
            accessibilityGranted
              ? "border border-success/30 opacity-70 bg-black/3 dark:bg-white/5 dark:border-success/25"
              : "border border-black/8 bg-black/3 dark:bg-white/5 dark:border-white/10"
          }`}
        >
          <div className="flex items-start gap-3 mb-3">
            <span className="text-2xl leading-none shrink-0 mt-0.5">♿</span>
            <div>
              <h3 className="text-[0.95rem] font-semibold m-0 mb-1">Accessibility</h3>
              <span
                className={`text-[0.7rem] font-semibold px-2 py-0.5 rounded-full inline-block tracking-tight ${
                  accessibilityGranted
                    ? "bg-green-100 text-green-800 dark:bg-success/15 dark:text-success"
                    : "bg-amber-100 text-amber-800 dark:bg-amber-500/15 dark:text-amber-300"
                }`}
              >
                {accessibilityGranted ? "Granted" : "Not Granted"}
              </span>
            </div>
          </div>
          {!accessibilityGranted && (
            <>
              <p className="text-sm leading-relaxed text-neutral-600 dark:text-neutral-400 mb-3">
                Accessibility access lets TuckNotes detect when you join a meeting
                so it can prompt you to start recording automatically.
              </p>
              <button
                className="border-[1.5px] border-primary dark:border-blue-400 text-primary dark:text-blue-400 bg-transparent rounded-xl py-2 px-6 text-sm font-semibold cursor-pointer transition-all duration-200 w-full mt-2 hover:bg-primary/8 dark:hover:bg-blue-400/10"
                onClick={handleAccessibilityEnable}
              >
                {accessibilityRequested
                  ? "Open System Settings"
                  : "Enable Accessibility"}
              </button>
              {accessibilityRequested && (
                <p className="text-xs text-neutral-400 dark:text-neutral-500 mt-4 leading-relaxed">
                  Find <strong>TuckNotes</strong> in the list and toggle it on.
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
