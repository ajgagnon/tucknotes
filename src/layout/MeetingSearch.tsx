import { useEffect, useState } from "react";
import { Search } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { type MeetingRow } from "@/features/meetings";

/** Sidebar search trigger + Cmd/Ctrl-K command palette over the meetings list. */
export function MeetingSearch({
  meetings,
  onSelectMeeting,
}: {
  meetings: MeetingRow[];
  onSelectMeeting: (meetingId: string) => void;
}) {
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
    <>
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
        <span className="min-w-0 flex-1 truncate text-xs">Search...</span>
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
                  onSelectMeeting(m.id);
                  setSearchOpen(false);
                }}
              >
                {m.title || "Recording"}
              </CommandItem>
            ))}
          </CommandGroup>
        </CommandList>
      </CommandDialog>
    </>
  );
}
