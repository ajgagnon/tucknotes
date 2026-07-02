import { Mic } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  useRecording,
  useAudioLevels,
  AudioVisualizer,
} from "@/features/recording";
import { type MeetingRow } from "@/features/meetings";
import { useLicenseStatus, allowsPaidFeatures } from "@/features/licensing";

/** Global start / navigate-to-active-recording button in the sidebar header. */
export function HeaderControls({
  meetings,
  onStartRecording,
  onNavigateToActiveRecording,
  onOpenSettings,
}: {
  meetings: MeetingRow[];
  onStartRecording: (meetingId: string) => void;
  onNavigateToActiveRecording: (meetingId: string) => void;
  onOpenSettings: () => void;
}) {
  const { recording, paused, startRecording, meetingId } = useRecording();
  const { systemLevel, micLevel } = useAudioLevels();
  const { status: licenseStatus } = useLicenseStatus();
  const sessionActive = recording || paused;
  const levelsInButton = recording && !paused;
  const entitled = allowsPaidFeatures(licenseStatus);

  const activeTitle =
    meetingId != null
      ? meetings.find((m) => m.id === meetingId)?.title?.trim() || "Recording"
      : "Recording";

  const handleClick = async () => {
    if (sessionActive) {
      if (meetingId != null) {
        onNavigateToActiveRecording(meetingId);
      }
      return;
    }
    if (!entitled) {
      // Trial expired or license invalid — route to settings instead of
      // failing silently when start_recording rejects.
      onOpenSettings();
      return;
    }
    try {
      const id = await startRecording();
      onStartRecording(id);
    } catch {
      // Error is already set in context by startRecording
    }
  };

  return (
    <Button
      variant={sessionActive ? "outline" : "default"}
      className="w-full justify-start gap-2 rounded-full"
      title={sessionActive ? activeTitle : undefined}
      onClick={() => void handleClick()}
    >
      {levelsInButton ? (
        <AudioVisualizer
          systemLevel={systemLevel}
          micLevel={micLevel}
          barClassName="bg-danger"
        />
      ) : (
        <Mic className="size-3.5 shrink-0" />
      )}
      <span className="truncate">
        {sessionActive ? activeTitle : "New Meeting"}
      </span>
    </Button>
  );
}
