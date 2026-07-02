import { useState } from "react";
import { MoreVertical, Pencil, Trash2 } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useRecording } from "@/features/recording";
import { type MeetingTitleInfo } from "@/features/meetings";
import { MeetingDateBadge } from "@/features/meetings/MeetingDateBadge";

/** Editable meeting title shown in the page header, with the rename/delete menu. */
export function MeetingHeaderTitle({
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
