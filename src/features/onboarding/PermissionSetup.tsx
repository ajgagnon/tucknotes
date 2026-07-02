import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Accessibility,
  CheckCircle2,
  Mic,
  Monitor,
  type LucideIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import OnboardingShell from "./OnboardingShell";

interface PermissionSetupProps {
  onComplete: () => void;
}

type MicStatus =
  "not_determined" | "authorized" | "denied" | "restricted" | "unknown";

type PermissionKey = "screen" | "mic" | "accessibility";
const ORDER: PermissionKey[] = ["screen", "mic", "accessibility"];

interface PermissionCopy {
  icon: LucideIcon;
  title: string;
  description: string;
  ctaEnable: string;
  hint: string;
}

const COPY: Record<PermissionKey, PermissionCopy> = {
  screen: {
    icon: Monitor,
    title: "Allow Screen Recording",
    description:
      "macOS requires Screen Recording to capture system audio. No video or screenshots are ever taken.",
    ctaEnable: "Enable Screen Recording",
    hint: "Find TuckNotes in the list and toggle it on.",
  },
  mic: {
    icon: Mic,
    title: "Allow Microphone Access",
    description:
      "Used to capture your voice during meetings. Audio is processed locally and never leaves your device.",
    ctaEnable: "Enable Microphone",
    hint: "Toggle TuckNotes on in System Settings → Privacy & Security → Microphone.",
  },
  accessibility: {
    icon: Accessibility,
    title: "Allow Accessibility Access",
    description:
      "Lets TuckNotes detect when you join a meeting so it can prompt you to start recording.",
    ctaEnable: "Enable Accessibility",
    hint: "Find TuckNotes in the list and toggle it on.",
  },
};

function PermissionSetup({ onComplete }: PermissionSetupProps) {
  const [screenGranted, setScreenGranted] = useState(false);
  const [micStatus, setMicStatus] = useState<MicStatus>("not_determined");
  const [accessibilityGranted, setAccessibilityGranted] = useState(false);
  const [screenRequested, setScreenRequested] = useState(false);
  const [micRequesting, setMicRequesting] = useState(false);
  const [accessibilityRequested, setAccessibilityRequested] = useState(false);
  const [loading, setLoading] = useState(true);
  const [stepIndex, setStepIndex] = useState(0);
  const initialAdvanceDone = useRef(false);

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

      if (!initialAdvanceDone.current) {
        initialAdvanceDone.current = true;
        const grants: Record<PermissionKey, boolean> = {
          screen,
          mic: mic === "authorized",
          accessibility,
        };
        const firstUngrantedIdx = ORDER.findIndex((key) => !grants[key]);
        setStepIndex(
          firstUngrantedIdx === -1 ? ORDER.length : firstUngrantedIdx,
        );
      }
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
      const result = await invoke<boolean>(
        "request_screen_recording_permission",
      );
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

  if (stepIndex >= ORDER.length) {
    return (
      <OnboardingShell
        icon={<CheckCircle2 strokeWidth={1.75} />}
        title="You're all set!"
        description="TuckNotes is ready to capture your meetings."
      >
        <Button
          className="w-full h-11 rounded-xl text-[0.95rem] font-semibold"
          onClick={onComplete}
        >
          Get Started
        </Button>
      </OnboardingShell>
    );
  }

  const key = ORDER[stepIndex];
  const copy = COPY[key];
  const Icon = copy.icon;

  const granted =
    key === "screen"
      ? screenGranted
      : key === "mic"
        ? micStatus === "authorized"
        : accessibilityGranted;

  const requested =
    key === "screen"
      ? screenRequested
      : key === "mic"
        ? micStatus === "denied" || micStatus === "restricted"
        : accessibilityRequested;

  const waiting = key === "mic" && micRequesting;

  let ctaLabel: string;
  if (granted) ctaLabel = "Continue";
  else if (waiting) ctaLabel = "Waiting for response…";
  else if (requested) ctaLabel = "Open System Settings";
  else ctaLabel = copy.ctaEnable;

  function handleCta() {
    if (granted) {
      setStepIndex((i) => i + 1);
      return;
    }
    if (key === "screen") return handleScreenEnable();
    if (key === "mic") return handleMicEnable();
    return handleAccessibilityEnable();
  }

  return (
    <OnboardingShell
      icon={<Icon strokeWidth={1.75} />}
      title={copy.title}
      description={copy.description}
      step={{ index: stepIndex, total: ORDER.length }}
    >
      <div className="flex flex-col items-center gap-3 w-full">
        <div className="flex items-center gap-2 text-xs font-medium">
          <span
            className={`size-1.5 rounded-full ${
              granted ? "bg-primary" : "bg-muted-foreground/40"
            }`}
          />
          <span className={granted ? "text-primary" : "text-muted-foreground"}>
            {granted ? "Granted" : "Not granted yet"}
          </span>
        </div>

        <Button
          className="w-full h-11 rounded-xl text-[0.95rem] font-semibold"
          onClick={handleCta}
          disabled={waiting}
        >
          {ctaLabel}
        </Button>

        {requested && !granted && (
          <p className="text-xs text-muted-foreground leading-relaxed">
            {copy.hint}
          </p>
        )}
      </div>
    </OnboardingShell>
  );
}

export default PermissionSetup;
