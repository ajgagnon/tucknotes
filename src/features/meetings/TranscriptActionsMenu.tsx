import { useCallback, useEffect, useRef, useState } from "react";
import {
  AlignLeft,
  Captions,
  Check,
  Clock,
  Copy,
  Download,
  Users,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { MeetingRow } from "./types";
import {
  buildExportContent,
  exportFilenameBase,
  formatTranscript,
  type TranscriptLine,
} from "./exportTranscript";

/** Copy/export actions for a meeting transcript (live or persisted). */
export function TranscriptActionsMenu({
  meeting,
  segments,
  className,
}: {
  meeting: MeetingRow;
  segments: TranscriptLine[];
  className?: string;
}) {
  const hasTranscript = segments.length > 0;

  const [copied, setCopied] = useState(false);
  const copyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
    },
    [],
  );

  const handleCopy = useCallback(
    (opts: { timestamps: boolean; speakers: boolean }) => {
      void navigator.clipboard.writeText(formatTranscript(segments, opts));
      setCopied(true);
      if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
      copyTimerRef.current = setTimeout(() => setCopied(false), 1500);
    },
    [segments],
  );

  const handleExport = useCallback(async () => {
    const base = exportFilenameBase(meeting);
    const path = await save({
      defaultPath: `${base}.md`,
      filters: [
        { name: "Markdown", extensions: ["md"] },
        { name: "Text", extensions: ["txt"] },
      ],
    });
    if (!path) return;
    await invoke("write_text_file", {
      path,
      contents: buildExportContent(meeting, segments),
    });
  }, [meeting, segments]);

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger
          render={
            <DropdownMenuTrigger
              render={
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  className={className}
                  aria-label="Copy or export transcript"
                  disabled={!hasTranscript}
                />
              }
            />
          }
        >
          {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
        </TooltipTrigger>
        <TooltipContent>Copy or export transcript</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="start" className="min-w-60">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Copy transcript</DropdownMenuLabel>
          <DropdownMenuItem
            onClick={() => handleCopy({ timestamps: false, speakers: false })}
          >
            <AlignLeft />
            Plain text
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => handleCopy({ timestamps: false, speakers: true })}
          >
            <Users />
            With speakers
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => handleCopy({ timestamps: true, speakers: false })}
          >
            <Clock />
            With timestamps
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => handleCopy({ timestamps: true, speakers: true })}
          >
            <Captions />
            With timestamps &amp; speakers
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          onClick={() => {
            void handleExport().catch((e) =>
              console.error("export transcript:", e),
            );
          }}
        >
          <Download />
          Export to file…
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
