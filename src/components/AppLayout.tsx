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
import RecordingView from "./RecordingView";
import TranscriptsView from "./TranscriptsView";
import SettingsView from "./SettingsView";
import { Button } from "./ui/button";

type Page = "recording" | "transcripts" | "settings";

const navItems = [
  { id: "recording" as const, label: "Recording", icon: Mic },
  { id: "transcripts" as const, label: "Transcripts", icon: FileText },
  { id: "settings" as const, label: "Settings", icon: Settings },
];

function AppLayout() {
  const [activePage, setActivePage] = useState<Page>("recording");
  const onDrag = useCallback((e: React.MouseEvent) => {
    if (e.button === 0 && e.detail === 1) {
      e.preventDefault();
      getCurrentWindow().startDragging();
    }
  }, []);

  return (
    <TooltipProvider>
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
            <div className="flex items-center justify-end p-3">
              <Button variant="default" className="rounded-full">
                Start Recording
              </Button>
            </div>
          </div>
          <div className="flex-1 overflow-auto">
            {activePage === "recording" && <RecordingView />}
            {activePage === "transcripts" && <TranscriptsView />}
            {activePage === "settings" && <SettingsView />}
          </div>
        </SidebarInset>
      </SidebarProvider>
    </TooltipProvider>
  );
}

export default AppLayout;
