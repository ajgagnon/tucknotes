import { useState, useCallback, useEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "@/hooks/use-tauri-event";
import { ask } from "@tauri-apps/plugin-dialog";
import { Settings } from "lucide-react";
import {
  SidebarProvider,
  Sidebar,
  SidebarHeader,
  SidebarFooter,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
  SidebarRail,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Toaster } from "@/components/ui/sonner";
import { Chatbot } from "@/components/chatbot/Chatbot";
import { AskTuckProvider } from "@/components/chatbot/ask-tuck-context";
import { RecordingProvider } from "@/features/recording";
import {
  MeetingsSidebar,
  type MeetingRow,
  type MeetingTitleInfo,
} from "@/features/meetings";
import { autoSummarizeIfNeeded } from "@/features/meetings/auto-summarize";
import { LlmDownloadIndicator } from "@/features/models";
import { TrialBanner } from "@/features/licensing";
import { HeaderControls } from "./HeaderControls";
import { MeetingSearch } from "./MeetingSearch";
import { LayoutContent, type ActiveView } from "./LayoutContent";

const appWindow = getCurrentWindow();

function AppLayout() {
  const [activeView, setActiveView] = useState<ActiveView>(null);
  const [meetings, setMeetings] = useState<MeetingRow[]>([]);
  const [meetingInfo, setMeetingInfo] = useState<MeetingTitleInfo | null>(null);

  // Tracks whether the template editor has unsaved edits. Set via onDirtyChange.
  const unsavedRef = useRef(false);

  // Confirms discarding unsaved template edits before leaving the editor.
  // Returns true if navigation may proceed.
  const confirmDiscardIfNeeded = useCallback(async () => {
    if (!unsavedRef.current) return true;
    const ok = await ask(
      "You have unsaved changes to this template. Discard them?",
      { title: "Discard changes?", kind: "warning" },
    );
    if (ok) unsavedRef.current = false;
    return ok;
  }, []);

  // Single entry point for user-initiated navigation, so the unsaved-changes
  // guard runs no matter how the user leaves the template editor.
  const navigateTo = useCallback(
    async (next: ActiveView) => {
      if (await confirmDiscardIfNeeded()) setActiveView(next);
    },
    [confirmDiscardIfNeeded],
  );

  // Only start window drag on primary-button single-click
  const onDrag = useCallback((e: React.MouseEvent) => {
    if (e.button === 0 && e.detail === 1) {
      e.preventDefault();
      appWindow.startDragging();
    }
  }, []);

  // Load meetings list
  const loadMeetings = useCallback(async () => {
    try {
      const result = await invoke<MeetingRow[]>("list_meetings");
      setMeetings(result);
    } catch (e) {
      console.error("Failed to load meetings:", e);
    }
  }, []);

  useEffect(() => {
    loadMeetings();
  }, [loadMeetings]);

  // Open latest meeting when nothing is selected (list_meetings is created_at DESC)
  useEffect(() => {
    if (activeView !== null) return;
    if (meetings.length === 0) return;
    setActiveView({ type: "meeting", id: meetings[0].id });
  }, [meetings, activeView]);

  // Refresh meetings list when a title is generated (so sidebar shows it)
  useTauriEvent("summary:title", () => {
    loadMeetings();
  });

  // Keep sidebar in sync with recording start/stop (e.g. meeting-detected overlay),
  // and navigate to the recording page whenever a new recording session begins —
  // including sessions started from other windows such as the meeting-detected overlay.
  const lastRecordingMeetingIdRef = useRef<string | null>(null);
  useTauriEvent<{
    recording: boolean;
    meeting_id: string | null;
  }>("recording-state-changed", ({ recording, meeting_id }) => {
    // Load meetings list to update the sidebar.
    loadMeetings();

    // No meeting attached — clear the tracker so the next session navigates.
    if (meeting_id == null) {
      lastRecordingMeetingIdRef.current = null;
      return;
    }
    // Only navigate to meeting if we are recording and also the meeting
    // is different from the last one we navigated to.
    if (!recording || meeting_id === lastRecordingMeetingIdRef.current) return;
    lastRecordingMeetingIdRef.current = meeting_id;
    setActiveView({ type: "meeting", id: meeting_id });
  });

  useTauriEvent<{ meeting_id: string }>("recording-finalized", (payload) => {
    loadMeetings();
    void autoSummarizeIfNeeded(payload.meeting_id);
  });

  const handleStartRecording = useCallback(
    (meetingId: string) => {
      setActiveView({ type: "meeting", id: meetingId });
      loadMeetings();
    },
    [loadMeetings],
  );

  const handleNavigateToActiveRecording = useCallback((meetingId: string) => {
    setActiveView({ type: "meeting", id: meetingId });
  }, []);

  const handleDeleteMeeting = useCallback(
    async (meetingId: string) => {
      const confirmed = await ask("This action cannot be undone.", {
        title: "Delete meeting?",
        kind: "warning",
      });
      if (!confirmed) return;
      try {
        await invoke("delete_meeting", { meetingId });
        setMeetings((prev) => prev.filter((m) => m.id !== meetingId));
        if (activeView?.type === "meeting" && activeView.id === meetingId) {
          setActiveView(null);
          setMeetingInfo(null);
        }
      } catch (e) {
        console.error("Failed to delete meeting:", e);
      }
    },
    [activeView],
  );

  const handleTitleChange = useCallback((info: MeetingTitleInfo) => {
    setMeetingInfo(info);
  }, []);

  const handleSaveTitle = useCallback(
    async (title: string) => {
      if (!activeView || activeView.type !== "meeting") return;
      setMeetingInfo((prev) => (prev ? { ...prev, title } : prev));
      try {
        await invoke("update_meeting_title", {
          meetingId: activeView.id,
          title,
        });
        loadMeetings();
      } catch (e) {
        console.error("Failed to update title:", e);
      }
    },
    [activeView, loadMeetings],
  );

  // Clear meeting info when navigating away from a meeting
  useEffect(() => {
    if (activeView?.type !== "meeting") {
      setMeetingInfo(null);
    }
  }, [activeView]);

  return (
    <div className="bg-background/50">
      <Toaster />
      <TooltipProvider>
       <AskTuckProvider>
        <RecordingProvider>
          <SidebarProvider className="max-h-svh overflow-hidden">
            <Sidebar variant="inset">
              <SidebarRail />
              <SidebarTrigger className="fixed left-[95px] top-[18px] text-muted-foreground/60 hover:text-muted-foreground" />
              <SidebarHeader
                className="flex flex-col gap-2 px-3 pb-3 pt-[55px]"
                onMouseDown={onDrag}
              >
                <div className="flex flex-col gap-2">
                  <div onMouseDown={(e) => e.stopPropagation()}>
                    <HeaderControls
                      meetings={meetings}
                      onStartRecording={handleStartRecording}
                      onNavigateToActiveRecording={
                        handleNavigateToActiveRecording
                      }
                      onOpenSettings={() =>
                        void navigateTo({ type: "settings" })
                      }
                    />
                  </div>
                  <MeetingSearch
                    meetings={meetings}
                    onSelectMeeting={(id) =>
                      void navigateTo({ type: "meeting", id })
                    }
                  />
                </div>
              </SidebarHeader>
              <MeetingsSidebar
                meetings={meetings}
                activeMeetingId={
                  activeView?.type === "meeting" ? activeView.id : null
                }
                onSelectMeeting={(id) =>
                  void navigateTo({ type: "meeting", id })
                }
              />

              <SidebarFooter className="px-3 pb-3">
                <LlmDownloadIndicator />
                <TrialBanner
                  onOpenSettings={() => void navigateTo({ type: "settings" })}
                />
                <SidebarMenu>
                  <SidebarMenuItem>
                    <SidebarMenuButton
                      isActive={activeView?.type === "settings"}
                      onClick={() => void navigateTo({ type: "settings" })}
                      tooltip="Settings"
                    >
                      <Settings className="text-muted-foreground" />
                      <span>Settings</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarFooter>
            </Sidebar>
            <LayoutContent
              activeView={activeView}
              onDrag={onDrag}
              onStartRecording={handleStartRecording}
              onDeleteMeeting={handleDeleteMeeting}
              onTitleChange={handleTitleChange}
              meetingInfo={meetingInfo}
              onSaveTitle={handleSaveTitle}
              onOpenSettings={(section) =>
                void navigateTo({ type: "settings", section })
              }
              onEditTemplate={(id) =>
                void navigateTo({ type: "template-editor", id })
              }
              onDirtyChange={(d) => (unsavedRef.current = d)}
            />
          </SidebarProvider>
        </RecordingProvider>
        <Chatbot
          activeMeeting={
            activeView?.type === "meeting"
              ? meetings.find((m) => m.id === activeView.id) ?? null
              : null
          }
          onOpenSettings={() => void navigateTo({ type: "settings" })}
          onOpenMeeting={(id) => void navigateTo({ type: "meeting", id })}
        />
       </AskTuckProvider>
      </TooltipProvider>
    </div>
  );
}

export default AppLayout;
