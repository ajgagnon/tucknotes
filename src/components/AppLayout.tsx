import { useState, useCallback, useEffect, useMemo, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  FileText,
  Mic,
  Settings,
  Clock,
  Trash2,
  MoreVertical,
  Search,
} from "lucide-react";
import {
  SidebarProvider,
  Sidebar,
  SidebarHeader,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarGroupContent,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
  SidebarInset,
  SidebarTrigger,
  useSidebar,
} from "@/components/ui/sidebar";
import { cn } from "@/lib/utils";
import { TooltipProvider } from "@/components/ui/tooltip";
import {
  RecordingProvider,
  useRecording,
  useAudioLevels,
} from "@/hooks/useRecording";
import MeetingsView, {
  type MeetingRow,
  type MeetingTitleInfo,
  type SummarizationQueue,
} from "./MeetingView";
import SettingsView from "./SettingsView";
import AudioVisualizer from "./AudioVisualizer";
import { Button } from "./ui/button";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "./ui/command";

type ActiveView =
  | { type: "meeting"; id: string }
  | { type: "settings" }
  | null;

const appWindow = getCurrentWindow();

// ---------------------------------------------------------------------------
// Date grouping helpers
// ---------------------------------------------------------------------------

type DateBucket = "Today" | "Yesterday" | "Older";
type SidebarGroupLabel = DateBucket | "Recents";

function getDateGroup(ms: number): DateBucket {
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const yesterday = new Date(today.getTime() - 86400000);
  const date = new Date(ms);
  if (date >= today) return "Today";
  if (date >= yesterday) return "Yesterday";
  return "Older";
}

function groupMeetings(
  meetings: MeetingRow[],
): { label: SidebarGroupLabel; meetings: MeetingRow[] }[] {
  const groups: Record<DateBucket, MeetingRow[]> = {
    Today: [],
    Yesterday: [],
    Older: [],
  };
  for (const m of meetings) {
    groups[getDateGroup(m.created_at)].push(m);
  }
  const order: DateBucket[] = ["Today", "Yesterday", "Older"];
  const result: { label: SidebarGroupLabel; meetings: MeetingRow[] }[] = order
    .filter((label) => groups[label].length > 0)
    .map((label) => ({ label, meetings: groups[label] }));
  // When "Older" is the only section, display it as "Recents"
  if (result.length === 1 && result[0].label === "Older") {
    result[0] = { ...result[0], label: "Recents" };
  }
  return result;
}

// ---------------------------------------------------------------------------
// Header controls (global start / navigate-to-active-recording)
// ---------------------------------------------------------------------------

function HeaderControls({
  meetings,
  onStartRecording,
  onNavigateToActiveRecording,
}: {
  meetings: MeetingRow[];
  onStartRecording: (meetingId: string) => void;
  onNavigateToActiveRecording: (meetingId: string) => void;
}) {
  const { recording, paused, startRecording, meetingId } = useRecording();
  const { systemLevel, micLevel } = useAudioLevels();
  const sessionActive = recording || paused;
  const levelsInButton = recording && !paused;

  const activeTitle =
    meetingId != null
      ? (meetings.find((m) => m.id === meetingId)?.title?.trim() ||
          "Untitled")
      : "Untitled";

  const handleClick = async () => {
    if (sessionActive) {
      if (meetingId != null) {
        onNavigateToActiveRecording(meetingId);
      }
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
    <div className="min-h-[35px] shrink-0 border-b border-muted-foreground/10" onMouseDown={onDrag}>
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

  if (!info) return null;

  if (editing) {
    return (
      <div
        className="min-w-0 flex-1"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <input
          className="text-sm font-semibold bg-transparent border-b border-primary outline-none w-full"
          defaultValue={info.title || ""}
          placeholder="Untitled"
          autoFocus
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
      </div>
    );
  }

  return (
    <div className="min-w-0 flex-1">
      <div className="flex items-center gap-2 min-w-0">
        <h1
          className="text-md font-semibold truncate cursor-text m-0"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => {
            if (!info.generatingTitle) setEditing(true);
          }}
          title="Click to rename"
        >
          {info.generatingTitle ? (
            <span className="flex items-center gap-2">
              <span className="text-muted-foreground">{info.title || "Generating title…"}</span>
              <span className="inline-block w-1.5 h-1.5 rounded-full bg-muted-foreground animate-pulse" />
            </span>
          ) : (
            info.title || "Untitled"
          )}
        </h1>
        {/* <span className="text-[10px] text-muted-foreground whitespace-nowrap shrink-0">
          {formatDate(info.createdAt)}
          {info.durationMs != null &&
            ` · ${formatTime(Math.floor(info.durationMs / 1000))}`}
        </span> */}
        <MeetingMenu onDelete={() => onDeleteMeeting(meetingId)} />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Meeting menu (three-dot dropdown with delete)
// ---------------------------------------------------------------------------

function MeetingMenu({ onDelete }: { onDelete: () => void }) {
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  // Close menu on outside click
  useEffect(() => {
    if (!menuOpen) return;
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [menuOpen]);

  return (
    <div
      className="relative"
      ref={menuRef}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <button
        onClick={() => setMenuOpen((prev) => !prev)}
        className="p-1.5 rounded-md text-neutral-400 hover:bg-black/5 dark:hover:bg-white/5 transition-colors cursor-pointer"
      >
        <MoreVertical className="w-4 h-4" />
      </button>
      {menuOpen && (
        <div className="absolute right-0 top-full mt-1 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg shadow-lg py-1 z-10 min-w-[140px]">
          <button
            onClick={() => {
              setMenuOpen(false);
              onDelete();
            }}
            className="flex items-center gap-2 w-full px-3 py-1.5 text-sm text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-500/10 transition-colors cursor-pointer"
          >
            <Trash2 className="w-3.5 h-3.5" />
            Delete
          </button>
        </div>
      )}
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
}: {
  activeView: ActiveView;
  onDrag: (e: React.MouseEvent) => void;
  onStartRecording: (meetingId: string) => void;
  onDeleteMeeting: (meetingId: string) => void;
  onTitleChange: (info: MeetingTitleInfo) => void;
  meetingInfo: MeetingTitleInfo | null;
  onSaveTitle: (title: string) => void;
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
      <h1 className="text-sm font-semibold px-2 m-0">Settings</h1>
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
  const [summaryQueue, setSummaryQueue] = useState<SummarizationQueue>({
    active: null,
    pending: [],
  });

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

  // Track summarization queue
  useEffect(() => {
    let cancelled = false;
    async function check() {
      try {
        const queue = await invoke<SummarizationQueue>(
          "get_summarization_queue",
        );
        if (!cancelled) setSummaryQueue(queue);
      } catch {
        /* ignore */
      }
    }
    check();

    const unlisten = listen<string>("summary:complete", () => {
      if (!cancelled) check();
    });
    return () => {
      cancelled = true;
      unlisten.then((fn) => fn());
    };
  }, []);

  // Refresh meetings list when a title is generated (so sidebar shows it)
  useEffect(() => {
    const unlisten = listen("summary:title", () => {
      loadMeetings();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadMeetings]);

  const grouped = useMemo(() => groupMeetings(meetings), [meetings]);

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
        if (
          activeView?.type === "meeting" &&
          activeView.id === meetingId
        ) {
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
      setMeetingInfo((prev) => prev ? { ...prev, title } : prev);
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
      setSearchOpen((open) => {
        if (open) return false;
        const t = e.target as HTMLElement;
        if (
          t.tagName === "INPUT" ||
          t.tagName === "TEXTAREA" ||
          t.isContentEditable
        ) {
          return false;
        }
        return true;
      });
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
                      onNavigateToActiveRecording={handleNavigateToActiveRecording}
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
                    <Search className="size-3 shrink-0 opacity-70" aria-hidden />
                    <span className="min-w-0 flex-1 truncate text-xs">
                      Search...
                    </span>
                    <span className="pointer-events-none hidden shrink-0 items-center gap-0.5 sm:inline-flex">
                      <kbd className="text-muted-foreground rounded font-sans text-xs font-medium">
                      {isMac ? "⌘" : "Ctrl"}
                        K
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
                            keywords={[m.title || "Untitled"]}
                            onSelect={() => {
                              setActiveView({ type: "meeting", id: m.id });
                              setSearchOpen(false);
                            }}
                          >
                            {m.title || "Untitled"}
                          </CommandItem>
                        ))}
                      </CommandGroup>
                    </CommandList>
                  </CommandDialog>
                </div>
              </SidebarHeader>
              <SidebarContent>
                {grouped.map((group) => (
                  <SidebarGroup key={group.label} className="px-3 py-0">
                    <SidebarGroupLabel className="text-xs text-muted-foreground px-2">
                      {group.label}
                    </SidebarGroupLabel>
                    <SidebarGroupContent>
                      <SidebarMenu>
                        {group.meetings.map((meeting) => (
                          <SidebarMenuItem key={meeting.id}>
                            <SidebarMenuButton
                              isActive={
                                activeView?.type === "meeting" &&
                                activeView.id === meeting.id
                              }
                              onClick={() =>
                                setActiveView({
                                  type: "meeting",
                                  id: meeting.id,
                                })
                              }
                              tooltip={meeting.title || "Untitled"}
                            >
                              <span className="truncate flex items-center gap-1.5 text-xs">
                                {meeting.title || "Untitled"}
                                {summaryQueue.active === meeting.id && (
                                  <span className="inline-block w-1.5 h-1.5 rounded-full bg-primary animate-pulse shrink-0" />
                                )}
                                {summaryQueue.pending.includes(meeting.id) && (
                                  <Clock className="w-3 h-3 text-muted-foreground shrink-0" />
                                )}
                              </span>
                            </SidebarMenuButton>
                          </SidebarMenuItem>
                        ))}
                      </SidebarMenu>
                    </SidebarGroupContent>
                  </SidebarGroup>
                ))}
              </SidebarContent>
              <SidebarFooter className="px-3 pb-3">
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
            />
          </SidebarProvider>
        </RecordingProvider>
      </TooltipProvider>
    </div>
  );
}

export default AppLayout;
