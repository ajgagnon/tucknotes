import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Editor } from "@tiptap/react";
import { Check, Copy, MessageCircleQuestion } from "lucide-react";
import { SimpleEditor } from "@/editor/templates/simple/simple-editor";
import { setSummaryHover } from "@/editor/extensions/summary-hover-highlight";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useAskTuck } from "@/components/chatbot/ask-tuck-context";

import "./meeting-notes-editor.scss";

const DEBOUNCE_MS = 500;

/** Selector matching the top-level content blocks of the summary editor. */
const BLOCK_SELECTOR =
  ".ProseMirror > p, .ProseMirror > h1, .ProseMirror > h2, .ProseMirror > h3, .ProseMirror > h4, .ProseMirror li";

type HoveredBlock = {
  el: HTMLElement;
  rect: DOMRect;
  isTask: boolean;
  text: string;
};

function readBlockText(block: HTMLElement): string {
  // For task/list items the visible text lives in an inner p/div; the checkbox
  // is an <input> (not text), so innerText already excludes it.
  const inner = block.querySelector<HTMLElement>("div p, p");
  return (inner ?? block).innerText.trim();
}

interface MeetingDocumentEditorProps {
  documentId: string;
  initialBody: string | null;
  /** Keep parent meeting detail in sync after save so remounting (e.g. tab switch) hydrates fresh markdown. */
  onDocumentBodySaved?: (documentId: string, body: string) => void;
  /** Seconds since recording start to stamp new lines; null disables stamping. */
  stampElapsedSecs: number | null;
  /** Jump transcript to this meeting time (ms). */
  onSeekTranscript?: (timestampMs: number) => void;
  /** Extra className applied to the outer wrapper (e.g. `meeting-summary-prose`). */
  className?: string;
  /** Show per-block hover actions (Ask Tuck / Copy) — only for the AI summary. */
  summaryActions?: boolean;
}

export function MeetingDocumentEditor({
  documentId,
  initialBody,
  onDocumentBodySaved,
  stampElapsedSecs,
  onSeekTranscript,
  className,
  summaryActions = false,
}: MeetingDocumentEditorProps) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastPersistedRef = useRef<string>(initialBody ?? "");
  const latestRef = useRef<string>(initialBody ?? "");
  const meetingNote = useMemo(() => ({ stampElapsedSecs }), [stampElapsedSecs]);

  const containerRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<Editor | null>(null);
  const { openAskTuck } = useAskTuck();
  const [hovered, setHovered] = useState<HoveredBlock | null>(null);
  const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const handler = (e: MouseEvent) => {
      const block = (e.target as HTMLElement).closest("[data-elapsed]");
      if (!block) return;
      const rect = block.getBoundingClientRect();
      if (e.clientX < rect.right - 52) return;
      const secs = parseInt(block.getAttribute("data-elapsed")!, 10);
      if (!Number.isNaN(secs)) onSeekTranscript?.(secs * 1000);
    };
    el.addEventListener("click", handler);
    return () => el.removeEventListener("click", handler);
  }, [onSeekTranscript]);

  const cancelClose = useCallback(() => {
    if (closeTimerRef.current) {
      clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  }, []);

  const scheduleClose = useCallback(() => {
    cancelClose();
    closeTimerRef.current = setTimeout(() => {
      closeTimerRef.current = null;
      setHovered(null);
    }, 120);
  }, [cancelClose]);

  // Per-block hover actions for the AI summary: track the hovered top-level
  // block and surface a floating toolbar at its right edge.
  useEffect(() => {
    if (!summaryActions) return;
    const el = containerRef.current;
    if (!el) return;

    const onOver = (e: MouseEvent) => {
      const block = (e.target as HTMLElement).closest<HTMLElement>(
        BLOCK_SELECTOR,
      );
      if (!block) return;
      cancelClose();
      const text = readBlockText(block);
      if (!text) return;
      setHovered({
        el: block,
        rect: block.getBoundingClientRect(),
        isTask: !!block.closest('ul[data-type="taskList"]'),
        text,
      });
    };
    const onLeave = () => scheduleClose();
    // Clear on scroll rather than chasing the rect (avoids drift); the toolbar
    // reappears on the next hover.
    const onScroll = () => setHovered(null);

    el.addEventListener("mouseover", onOver);
    el.addEventListener("mouseleave", onLeave);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      el.removeEventListener("mouseover", onOver);
      el.removeEventListener("mouseleave", onLeave);
      window.removeEventListener("scroll", onScroll, true);
      cancelClose();
    };
  }, [summaryActions, cancelClose, scheduleClose]);

  // Drive the block's highlight band from `hovered` via a ProseMirror decoration
  // (a manual class would be stripped by PM's DOMObserver). Sourcing it from the
  // same state that keeps the toolbar alive lets the band persist while the cursor
  // is over the detached action toolbar. Deduped by element so intra-block
  // `mouseover` events don't dispatch a transaction each time.
  const activeDecoElRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    const editor = editorRef.current;
    if (!editor || editor.isDestroyed) return;
    const el = hovered?.el ?? null;
    if (el === activeDecoElRef.current) return;
    activeDecoElRef.current = el;
    setSummaryHover(editor, el);
  }, [hovered]);

  const persist = useCallback(
    async (markdown: string) => {
      if (markdown === lastPersistedRef.current) return;
      try {
        await invoke("update_meeting_document_body", {
          documentId,
          body: markdown,
        });
        lastPersistedRef.current = markdown;
        onDocumentBodySaved?.(documentId, markdown);
      } catch (e) {
        console.error("update_meeting_document_body:", e);
      }
    },
    [documentId, onDocumentBodySaved],
  );

  const onMarkdownChange = useCallback(
    (markdown: string) => {
      latestRef.current = markdown;
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => {
        timerRef.current = null;
        void persist(markdown);
      }, DEBOUNCE_MS);
    },
    [persist],
  );

  useEffect(
    () => () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      const pending = latestRef.current;
      if (pending !== lastPersistedRef.current) {
        onDocumentBodySaved?.(documentId, pending);
        void persist(pending);
      }
    },
    [documentId, onDocumentBodySaved, persist],
  );

  return (
    <div
      ref={containerRef}
      className={`meeting-notes-editor flex min-h-0 flex-1 flex-col${className ? ` ${className}` : ""}`}
    >
      <SimpleEditor
        key={documentId}
        initialMarkdown={initialBody}
        onMarkdownChange={onMarkdownChange}
        hideThemeToggle
        meetingNote={meetingNote}
        summaryHover={summaryActions}
        onEditorReady={(ed) => {
          editorRef.current = ed;
        }}
      />
      {summaryActions && hovered && (
        <SummaryBlockActions
          block={hovered}
          onAskTuck={() =>
            openAskTuck(`"${hovered.text}"\n\nCan you clarify this?`)
          }
          onMouseEnter={cancelClose}
          onMouseLeave={scheduleClose}
        />
      )}
    </div>
  );
}

function SummaryBlockActions({
  block,
  onAskTuck,
  onMouseEnter,
  onMouseLeave,
}: {
  block: HoveredBlock;
  onAskTuck: () => void;
  onMouseEnter: () => void;
  onMouseLeave: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const copyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
    },
    [],
  );

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(block.text);
    setCopied(true);
    if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
    copyTimerRef.current = setTimeout(() => setCopied(false), 1500);
  }, [block.text]);

  return (
    // Anchor the toolbar's right edge just inside the block's right edge, and
    // vertically center it on the block (top is the block's midpoint).
    <div
      className="fixed z-50 flex items-center gap-0.5 rounded-lg bg-popover p-0.5 text-popover-foreground [transform:translate(calc(-100%_-_0.25rem),-50%)]"
      style={{
        top: block.rect.top + block.rect.height / 2,
        left: block.rect.right,
      }}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
    >
      {block.isTask && (
        <Tooltip>
          <TooltipTrigger
            render={
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label="Copy task"
                onMouseDown={(e) => e.preventDefault()}
                onClick={handleCopy}
              />
            }
          >
            {copied ? <Check /> : <Copy />}
          </TooltipTrigger>
          <TooltipContent>{copied ? "Copied" : "Copy task"}</TooltipContent>
        </Tooltip>
      )}
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label="Ask Tuck about this"
              onMouseDown={(e) => e.preventDefault()}
              onClick={onAskTuck}
            />
          }
        >
          <MessageCircleQuestion />
        </TooltipTrigger>
        <TooltipContent>Ask Tuck about this</TooltipContent>
      </Tooltip>
    </div>
  );
}
