import { useState, useCallback, useEffect, useRef } from "react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  FileText,
  Mic,
  Settings,
  Trash2,
  MoreVertical,
  Search,
  Pencil,
} from "lucide-react";
import {
  SidebarProvider,
  Sidebar,
  SidebarHeader,
  SidebarFooter,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
  SidebarInset,
  SidebarTrigger,
  useSidebar,
} from "@/components/ui/sidebar";
import { cn } from "@/lib/utils";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Chatbot } from "@/components/chatbot/Chatbot";
import {
  RecordingProvider,
  useRecording,
  useAudioLevels,
  AudioVisualizer,
} from "@/features/recording";
import MeetingsView, {
  MeetingsSidebar,
  type MeetingRow,
  type MeetingTitleInfo,
} from "@/features/meetings";
import {
  type MeetingDetail,
  summaryBodyFromDocuments,
} from "@/features/meetings/types";
import { MeetingDateBadge } from "@/features/meetings/MeetingDateBadge";
import SettingsView from "@/features/settings/SettingsView";
import { LlmDownloadIndicator } from "@/features/models";
import {
  TrialBanner,
  useLicenseStatus,
  allowsPaidFeatures,
} from "@/features/licensing";
import { Button } from "@/components/ui/button";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";

type ActiveView = { type: "meeting"; id: string } | { type: "settings" } | null;

const appWindow = getCurrentWindow();

/// Kicks off summarization for a just-finalized recording when there's no
/// existing summary and an LLM model is downloaded. Lives at the layout level
/// so it fires regardless of which view (or window) is currently focused.
async function autoSummarizeIfNeeded(meetingId: string): Promise<void> {
  try {
    const detail = await invoke<MeetingDetail>("get_meeting", { meetingId });
    if (summaryBodyFromDocuments(detail.documents)) return;

    const selected = await invoke<string | null>("get_selected_llm_model");
    if (!selected) return;
    const ready = await invoke<boolean>("get_llm_model_status", {
      modelId: selected,
    });
    if (!ready) return;

    await invoke<string>("summarize_meeting", { meetingId });
  } catch (e) {
    // Already in progress / queued — not an error worth surfacing here.
    console.debug("autoSummarizeIfNeeded:", e);
  }
}

// ---------------------------------------------------------------------------
// Header controls (global start / navigate-to-active-recording)
// ---------------------------------------------------------------------------

function HeaderControls({
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

// ---------------------------------------------------------------------------
// Page header (reusable title bar / drag region)
// ---------------------------------------------------------------------------

function PageHeader({
  left,
  right,
  onDrag,
}: {
  left?: React.ReactNode;
  right?: React.ReactNode;
  onDrag: (e: React.MouseEvent) => void;
}) {
  const { state } = useSidebar();
  const needsStoplightPadding = state === "collapsed";

  return (
    <div
      className="min-h-[48px] shrink-0 border-b border-muted-foreground/10"
      onMouseDown={onDrag}
    >
      <div className="flex items-center justify-between px-5 py-2 pr-2 h-full gap-2">
        <div
          className={cn(
            "flex items-center gap-1 transition-all duration-200 ease-in-out min-w-0 flex-1",
            needsStoplightPadding && "pl-[110px]",
          )}
        >
          {left}
        </div>
        <div className="flex items-center gap-2 shrink-0">{right}</div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Meeting header title (editable, shown in PageHeader)
// ---------------------------------------------------------------------------

function MeetingHeaderTitle({
  info,
  onSave,
  meetingId,
  onDeleteMeeting,
}: {
  info: MeetingTitleInfo | null;
  onSave: (title: string) => void;
  meetingId: string;
  onDeleteMeeting: (meetingId: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const { recording, paused, meetingId: recordingMeetingId } = useRecording();
  const disableDelete =
    (recording || paused) && recordingMeetingId === meetingId;

  if (!info) return null;

  return (
    <div className="min-w-0 flex-1 flex items-center justify-between">
      <div className="flex items-center gap-2 min-w-0 flex-1">
        {editing ? (
          <input
            className="text-lg font-semibold font-serif bg-transparent border-b border-primary outline-none min-w-0 flex-1 m-0 p-0"
            defaultValue={info.title || ""}
            placeholder="Recording"
            autoFocus
            onMouseDown={(e) => e.stopPropagation()}
            onBlur={(e) => {
              setEditing(false);
              const trimmed = e.currentTarget.value.trim();
              if (trimmed && trimmed !== (info.title ?? "")) {
                onSave(trimmed);
              }
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.currentTarget.blur();
              } else if (e.key === "Escape") {
                setEditing(false);
              }
            }}
          />
        ) : (
          <h1
            className="text-lg font-semibold truncate cursor-text m-0"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() => {
              if (!info.generatingTitle) setEditing(true);
            }}
            title="Click to rename"
          >
            {info.generatingTitle ? (
              <span className="flex items-center gap-2">
                <span className="text-muted-foreground">
                  {info.title || "Generating title…"}
                </span>
                <span className="inline-block w-1.5 h-1.5 rounded-full bg-muted-foreground animate-pulse" />
              </span>
            ) : (
              info.title || "Recording"
            )}
          </h1>
        )}

        <MeetingMenu
          onDelete={() => onDeleteMeeting(meetingId)}
          onRename={() => setEditing(true)}
          disableRename={info.generatingTitle}
          disableDelete={disableDelete}
        />
      </div>
      <MeetingDateBadge
        title={info.title}
        createdAt={info.createdAt}
        durationMs={info.durationMs}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Meeting menu (three-dot dropdown with delete)
// ---------------------------------------------------------------------------

function MeetingMenu({
  onDelete,
  onRename,
  disableRename,
  disableDelete,
}: {
  onDelete: () => void;
  onRename: () => void;
  disableRename: boolean;
  disableDelete: boolean;
}) {
  return (
    <div onMouseDown={(e) => e.stopPropagation()}>
      <DropdownMenu>
        <DropdownMenuTrigger className="p-1.5 rounded-md text-neutral-400 hover:bg-black/5 dark:hover:bg-white/5 transition-colors cursor-pointer outline-none">
          <MoreVertical className="w-4 h-4" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="min-w-[140px]">
          <DropdownMenuItem onClick={onRename} disabled={disableRename}>
            <Pencil className="w-3.5 h-3.5" />
            Rename
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            onClick={onDelete}
            disabled={disableDelete}
            variant="destructive"
          >
            <Trash2 className="w-3.5 h-3.5" />
            Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main content area
// ---------------------------------------------------------------------------

function LayoutContent({
  activeView,
  onDrag,
  onStartRecording,
  onDeleteMeeting,
  onTitleChange,
  meetingInfo,
  onSaveTitle,
  onOpenSettings,
}: {
  activeView: ActiveView;
  onDrag: (e: React.MouseEvent) => void;
  onStartRecording: (meetingId: string) => void;
  onDeleteMeeting: (meetingId: string) => void;
  onTitleChange: (info: MeetingTitleInfo) => void;
  meetingInfo: MeetingTitleInfo | null;
  onSaveTitle: (title: string) => void;
  onOpenSettings: () => void;
}) {
  // Build header left content based on active view
  const headerLeft =
    activeView?.type === "meeting" ? (
      <MeetingHeaderTitle
        info={meetingInfo}
        onSave={onSaveTitle}
        meetingId={activeView.id}
        onDeleteMeeting={onDeleteMeeting}
      />
    ) : activeView?.type === "settings" ? (
      <h1 className="text-lg font-semibold m-0">Settings</h1>
    ) : null;

  return (
    <SidebarInset className="min-h-0 overflow-hidden">
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-background border border-muted-foreground/10 md:rounded-2xl">
        <PageHeader left={headerLeft} onDrag={onDrag} />
        <div className="min-h-0 flex-1 overflow-auto">
          {activeView?.type === "meeting" && (
            <MeetingsView
              meetingId={activeView.id}
              onTitleChange={onTitleChange}
              onRecordingStarted={onStartRecording}
              onOpenSettings={onOpenSettings}
            />
          )}
          {activeView?.type === "settings" && <SettingsView />}
          {activeView === null && (
            <div className="flex h-full flex-col items-center justify-center p-8 text-center">
              <FileText className="mb-4 h-12 w-12 text-muted-foreground" />
              <p className="text-sm text-muted-foreground">
                No meetings yet. Start a recording to create your first meeting.
              </p>
            </div>
          )}
        </div>
      </div>
    </SidebarInset>
  );
}

// ---------------------------------------------------------------------------
// App layout with sidebar meetings list
// ---------------------------------------------------------------------------

function AppLayout() {
  const [activeView, setActiveView] = useState<ActiveView>(null);
  const [meetings, setMeetings] = useState<MeetingRow[]>([]);
  const [meetingInfo, setMeetingInfo] = useState<MeetingTitleInfo | null>(null);

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
  useEffect(() => {
    const unlisten = listen("summary:title", () => {
      loadMeetings();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadMeetings]);

  // Keep sidebar in sync with recording start/stop (e.g. meeting-detected overlay),
  // and navigate to the recording page whenever a new recording session begins —
  // including sessions started from other windows such as the meeting-detected overlay.
  const lastRecordingMeetingIdRef = useRef<string | null>(null);
  useEffect(() => {
    const unlistenStateChanged = listen<{
      recording: boolean;
      meeting_id: string | null;
    }>("recording-state-changed", ({ payload: { recording, meeting_id } }) => {
      // Load meetings list to update the sidebar.
      loadMeetings();

      // No meeting attached — clear the tracker so the next session navigates.
      if (meeting_id == null) {
        lastRecordingMeetingIdRef.current = null;
        return;
      }
      // Only navigate to meeting if we are recording and also the meeting
      // is different from the last one we navigated to.
      if (!recording || meeting_id === lastRecordingMeetingIdRef.current)
        return;
      lastRecordingMeetingIdRef.current = meeting_id;
      setActiveView({ type: "meeting", id: meeting_id });
    });
    const unlistenFinalized = listen<{ meeting_id: string }>(
      "recording-finalized",
      (event) => {
        loadMeetings();
        void autoSummarizeIfNeeded(event.payload.meeting_id);
      },
    );
    return () => {
      unlistenStateChanged.then((fn) => fn());
      unlistenFinalized.then((fn) => fn());
    };
  }, [loadMeetings]);

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

  const [searchOpen, setSearchOpen] = useState(false);
  const isMac =
    typeof navigator !== "undefined" && /Mac/i.test(navigator.userAgent);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== "k") return;
      e.preventDefault();
      setSearchOpen((open) => !open);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="bg-background/50">
      <TooltipProvider>
        <RecordingProvider>
          <SidebarProvider className="max-h-svh overflow-hidden">
            <Sidebar variant="inset">
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
                      onOpenSettings={() => setActiveView({ type: "settings" })}
                    />
                  </div>
                  <button
                    type="button"
                    onClick={() => setSearchOpen(true)}
                    onMouseDown={(e) => e.stopPropagation()}
                    className={cn(
                      "flex h-8 w-full min-w-0 items-center gap-2 px-2 text-left text-sm text-muted-foreground hover:text-foreground transition-colors",
                    )}
                    aria-label="Search meetings"
                    aria-keyshortcuts={isMac ? "Meta+K" : "Control+K"}
                  >
                    <Search
                      className="size-3 shrink-0 opacity-70"
                      aria-hidden
                    />
                    <span className="min-w-0 flex-1 truncate text-xs">
                      Search...
                    </span>
                    <span className="pointer-events-none hidden shrink-0 items-center gap-0.5 sm:inline-flex">
                      <kbd className="text-muted-foreground rounded font-sans text-xs font-medium">
                        {isMac ? "⌘" : "Ctrl"}K
                      </kbd>
                    </span>
                  </button>
                  <CommandDialog
                    open={searchOpen}
                    onOpenChange={setSearchOpen}
                    label="Search meetings"
                  >
                    <div className="flex items-center gap-2 border-b border-border px-3">
                      <Search
                        className="size-4 shrink-0 text-muted-foreground"
                        aria-hidden
                      />
                      <CommandInput
                        placeholder="Search meetings…"
                        className="h-11 flex-1 border-0 focus-visible:ring-0"
                      />
                    </div>
                    <CommandList>
                      <CommandEmpty>No meetings found.</CommandEmpty>
                      <CommandGroup heading="Meetings">
                        {meetings.map((m) => (
                          <CommandItem
                            key={m.id}
                            value={m.id}
                            keywords={[m.title || "Recording"]}
                            onSelect={() => {
                              setActiveView({ type: "meeting", id: m.id });
                              setSearchOpen(false);
                            }}
                          >
                            {m.title || "Recording"}
                          </CommandItem>
                        ))}
                      </CommandGroup>
                    </CommandList>
                  </CommandDialog>
                </div>
              </SidebarHeader>
              <MeetingsSidebar
                meetings={meetings}
                activeMeetingId={
                  activeView?.type === "meeting" ? activeView.id : null
                }
                onSelectMeeting={(id) => setActiveView({ type: "meeting", id })}
              />

              <SidebarFooter className="px-3 pb-3">
                <LlmDownloadIndicator />
                <TrialBanner
                  onOpenSettings={() => setActiveView({ type: "settings" })}
                />
                <SidebarMenu>
                  <SidebarMenuItem>
                    <SidebarMenuButton
                      isActive={activeView?.type === "settings"}
                      onClick={() => setActiveView({ type: "settings" })}
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
              onOpenSettings={() => setActiveView({ type: "settings" })}
            />
          </SidebarProvider>
        </RecordingProvider>
        <Chatbot
          activeMeeting={
            activeView?.type === "meeting"
              ? meetings.find((m) => m.id === activeView.id) ?? null
              : null
          }
          onOpenSettings={() => setActiveView({ type: "settings" })}
          onOpenMeeting={(id) => setActiveView({ type: "meeting", id })}
        />
      </TooltipProvider>
    </div>
  );
}

export default AppLayout;
