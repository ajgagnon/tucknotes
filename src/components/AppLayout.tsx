import { useState, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { FileText, Settings } from "lucide-react";
import {
  SidebarProvider,
  Sidebar,
  SidebarHeader,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
  SidebarInset,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { TooltipProvider } from "@/components/ui/tooltip";
import {
  RecordingProvider,
  useRecording,
  useAudioLevels,
} from "@/hooks/useRecording";
import { formatTime } from "@/lib/formatTime";
import MeetingsView from "./MeetingsView";
import SettingsView from "./SettingsView";
import AudioVisualizer from "./AudioVisualizer";
import { Button } from "./ui/button";

type Page = "meetings" | "settings";

const navItems = [
  { id: "meetings" as const, label: "Meetings", icon: FileText },
  { id: "settings" as const, label: "Settings", icon: Settings },
];

const pageTitles: Record<Page, string> = {
  meetings: "Meetings",
  settings: "Settings",
};

const appWindow = getCurrentWindow();

function HeaderControls({
  onStartRecording,
}: {
  onStartRecording: (meetingId: string) => void;
}) {
  const { recording, startRecording, stopRecording, elapsed } = useRecording();
  const { systemLevel, micLevel } = useAudioLevels();

  const handleClick = async () => {
    if (recording) {
      await stopRecording();
    } else {
      try {
        const meetingId = await startRecording();
        onStartRecording(meetingId);
      } catch {
        // Error is already set in context by startRecording
      }
    }
  };

  return (
    <div className="flex items-center gap-2">
      {recording && (
        <>
          <AudioVisualizer systemLevel={systemLevel} micLevel={micLevel} />
          <span className="text-xs tabular-nums text-danger font-medium">
            {formatTime(elapsed)}
          </span>
        </>
      )}
      <Button
        variant={recording ? "destructive" : "default"}
        className="rounded-full"
        onClick={handleClick}
      >
        {recording ? "Stop Recording" : "Start Recording"}
      </Button>
    </div>
  );
}

function AppLayout() {
  const [activePage, setActivePage] = useState<Page>("meetings");
  const [activeMeetingId, setActiveMeetingId] = useState<string | null>(null);

  // Only start window drag on primary-button single-click (ignore double-click/right-click)
  const onDrag = useCallback((e: React.MouseEvent) => {
    if (e.button === 0 && e.detail === 1) {
      e.preventDefault();
      appWindow.startDragging();
    }
  }, []);

  const handleStartRecording = useCallback((meetingId: string) => {
    setActiveMeetingId(meetingId);
    setActivePage("meetings");
  }, []);

  const clearActiveMeeting = useCallback(() => {
    setActiveMeetingId(null);
  }, []);

  return (
    <TooltipProvider>
      <RecordingProvider>
        <SidebarProvider>
          <Sidebar variant="inset">
            <SidebarTrigger className="fixed left-[85px] top-[8px] text-muted-foreground/60 hover:text-muted-foreground" />
            <SidebarHeader
              className="h-[35px]"
              onMouseDown={onDrag}
            ></SidebarHeader>
            <SidebarContent>
              <SidebarGroup className="px-3">
                <SidebarGroupContent>
                  <SidebarMenu className="gap-1.5">
                    {navItems.map((item) => (
                      <SidebarMenuItem key={item.id}>
                        <SidebarMenuButton
                          size="lg"
                          isActive={activePage === item.id}
                          onClick={() => setActivePage(item.id)}
                          tooltip={item.label}
                        >
                          <item.icon className={"text-muted-foreground"} />
                          <span>{item.label}</span>
                        </SidebarMenuButton>
                      </SidebarMenuItem>
                    ))}
                  </SidebarMenu>
                </SidebarGroupContent>
              </SidebarGroup>
            </SidebarContent>
          </Sidebar>
          <SidebarInset>
            <div className="h-[35px] shrink-0" onMouseDown={onDrag}>
              <div className="flex items-center justify-between p-3">
                <h1 className="text-lg font-semibold px-2">
                  {pageTitles[activePage]}
                </h1>
                <HeaderControls onStartRecording={handleStartRecording} />
              </div>
            </div>
            <div className="flex-1 overflow-auto">
              {activePage === "meetings" && (
                <MeetingsView
                  activeMeetingId={activeMeetingId}
                  onClearActiveMeeting={clearActiveMeeting}
                />
              )}
              {activePage === "settings" && <SettingsView />}
            </div>
          </SidebarInset>
        </SidebarProvider>
      </RecordingProvider>
    </TooltipProvider>
  );
}

export default AppLayout;
