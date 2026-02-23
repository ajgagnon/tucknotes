import { useState, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Mic, FileText, Settings } from "lucide-react";
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
import { RecordingProvider, useRecording } from "@/hooks/useRecording";
import { formatTime } from "@/lib/formatTime";
import MeetingView from "./MeetingView";
import MeetingsView from "./MeetingsView";
import SettingsView from "./SettingsView";
import AudioVisualizer from "./AudioVisualizer";
import { Button } from "./ui/button";

type Page = "meeting" | "meetings" | "settings";

const navItems = [
  { id: "meeting" as const, label: "Meeting", icon: Mic },
  { id: "meetings" as const, label: "Meetings", icon: FileText },
  { id: "settings" as const, label: "Settings", icon: Settings },
];

function HeaderControls({
  onNavigateToMeeting,
}: {
  onNavigateToMeeting: () => void;
}) {
  const {
    recording,
    startRecording,
    stopRecording,
    elapsed,
    systemLevel,
    micLevel,
  } = useRecording();

  const handleClick = async () => {
    if (recording) {
      await stopRecording();
    } else {
      await startRecording();
      onNavigateToMeeting();
    }
  };

  return (
    <div className="flex items-center gap-2">
      {recording && (
        <AudioVisualizer systemLevel={systemLevel} micLevel={micLevel} />
      )}
      {recording && (
        <span className="text-xs tabular-nums text-danger font-medium">
          {formatTime(elapsed)}
        </span>
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
  const onDrag = useCallback((e: React.MouseEvent) => {
    if (e.button === 0 && e.detail === 1) {
      e.preventDefault();
      getCurrentWindow().startDragging();
    }
  }, []);

  return (
    <TooltipProvider>
      <RecordingProvider>
        <SidebarProvider>
          <Sidebar variant="inset">
            <SidebarTrigger className="fixed left-[100px] top-[8px] text-muted-foreground/60 hover:text-muted-foreground" />
            <SidebarHeader
              className="h-[50px]"
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
                          <item.icon
                            className={
                              activePage === item.id
                                ? "text-sidebar-accent-foreground"
                                : "text-muted-foreground"
                            }
                          />
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
            <div className="h-[50px] shrink-0" onMouseDown={onDrag}>
              <div className="flex items-center justify-between p-3">
                <h1 className="text-lg font-semibold px-2">
                  {activePage === "meeting" && "Meeting"}
                  {activePage === "meetings" && "Meetings"}
                  {activePage === "settings" && "Settings"}
                </h1>
                <HeaderControls
                  onNavigateToMeeting={() => setActivePage("meeting")}
                />
              </div>
            </div>
            <div className="flex-1 overflow-auto">
              {activePage === "meeting" && <MeetingView />}
              {activePage === "meetings" && <MeetingsView />}
              {activePage === "settings" && <SettingsView />}
            </div>
          </SidebarInset>
        </SidebarProvider>
      </RecordingProvider>
    </TooltipProvider>
  );
}

export default AppLayout;
