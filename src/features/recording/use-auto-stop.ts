import { useCallback, useRef, type RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "@/hooks/use-tauri-event";

interface MeetingDetectedEvent {
  phase: "Idle" | "Detecting" | "Active" | "Ending";
  app_name: string | null;
}

/**
 * The auto-stop prompt concern: tracks whether the current recording session
 * matched a detected meeting app, and shows the "meeting ended — stop
 * recording?" overlay when that meeting ends.
 */
export function useAutoStop(sessionActiveRef: RefObject<boolean>): {
  /** New session started: clear state, then recover an already-detected meeting app. */
  armForNewSession: () => void;
  /** Session ended: clear state and close any open prompt. */
  disarm: () => void;
  /** Close the prompt without clearing state (pause is an explicit "I'm not done"). */
  hideOverlay: () => void;
} {
  const matchedMeetingRef = useRef(false);
  const matchedAppNameRef = useRef<string | null>(null);
  // Once the user clicks "Keep" on the auto-stop prompt, suppress further
  // prompts for the rest of this recording session — don't nag on repeated
  // false positives from the AX-based detector.
  const autoStopDismissedRef = useRef(false);

  const hideOverlay = useCallback(() => {
    void invoke("hide_auto_stop_overlay").catch(() => {});
  }, []);

  const disarm = useCallback(() => {
    matchedMeetingRef.current = false;
    matchedAppNameRef.current = null;
    autoStopDismissedRef.current = false;
    hideOverlay();
  }, [hideOverlay]);

  const armForNewSession = useCallback(() => {
    disarm();
    // Recover state when recording starts after a meeting was already
    // detected — otherwise no `meeting-detected` event arrives during
    // this session and auto-stop would never arm.
    void invoke<string | null>("get_current_meeting_app")
      .then((appName) => {
        if (appName) {
          matchedMeetingRef.current = true;
          matchedAppNameRef.current = appName;
        }
      })
      .catch(() => {});
  }, [disarm]);

  useTauriEvent<MeetingDetectedEvent>("meeting-detected", (payload) => {
    if (!sessionActiveRef.current) return;
    matchedMeetingRef.current = true;
    matchedAppNameRef.current = payload.app_name;
    // Detector saw the call again — close any stale prompt (false positive
    // recovered, or user rejoined).
    hideOverlay();
  });

  useTauriEvent("auto-stop-cancel-requested", () => {
    autoStopDismissedRef.current = true;
  });

  useTauriEvent<MeetingDetectedEvent>("meeting-ended", () => {
    if (!sessionActiveRef.current) return;
    if (!matchedMeetingRef.current) return;
    if (autoStopDismissedRef.current) return;

    void invoke("show_auto_stop_overlay", {
      appName: matchedAppNameRef.current,
    }).catch((e) => console.error("show_auto_stop_overlay:", e));
  });

  return { armForNewSession, disarm, hideOverlay };
}
