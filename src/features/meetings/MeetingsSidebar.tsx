import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "@/hooks/use-tauri-event";
import { Clock } from "lucide-react";
import {
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarGroupContent,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
} from "@/components/ui/sidebar";
import type { MeetingRow, SummarizationQueue } from "./types";

type DateBucket = "Today" | "Yesterday" | "Older";
type GroupLabel = DateBucket | "Recents";

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
): { label: GroupLabel; meetings: MeetingRow[] }[] {
  const groups: Record<DateBucket, MeetingRow[]> = {
    Today: [],
    Yesterday: [],
    Older: [],
  };
  for (const m of meetings) {
    groups[getDateGroup(m.created_at)].push(m);
  }
  const order: DateBucket[] = ["Today", "Yesterday", "Older"];
  const result: { label: GroupLabel; meetings: MeetingRow[] }[] = order
    .filter((label) => groups[label].length > 0)
    .map((label) => ({ label, meetings: groups[label] }));
  if (result.length === 1 && result[0].label === "Older") {
    result[0] = { ...result[0], label: "Recents" };
  }
  return result;
}

export default function MeetingsSidebar({
  meetings,
  activeMeetingId,
  onSelectMeeting,
}: {
  meetings: MeetingRow[];
  activeMeetingId: string | null;
  onSelectMeeting: (id: string) => void;
}) {
  const [summaryQueue, setSummaryQueue] = useState<SummarizationQueue>({
    active: null,
    pending: [],
  });

  const queueMountedRef = useRef(true);
  const checkSummaryQueue = useCallback(async () => {
    try {
      const queue = await invoke<SummarizationQueue>("get_summarization_queue");
      if (queueMountedRef.current) setSummaryQueue(queue);
    } catch {
      /* ignore */
    }
  }, []);

  useEffect(() => {
    queueMountedRef.current = true;
    void checkSummaryQueue();
    return () => {
      queueMountedRef.current = false;
    };
  }, [checkSummaryQueue]);

  useTauriEvent("summary:complete", () => void checkSummaryQueue());

  const grouped = useMemo(() => groupMeetings(meetings), [meetings]);

  return (
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
                    isActive={activeMeetingId === meeting.id}
                    onClick={() => onSelectMeeting(meeting.id)}
                    tooltip={meeting.title || "Recording"}
                  >
                    <span className="truncate flex items-center gap-1.5 text-xs">
                      {meeting.title || "Recording"}
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
  );
}
