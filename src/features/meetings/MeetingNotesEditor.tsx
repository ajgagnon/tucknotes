import { useCallback, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SimpleEditor } from "@/editor/templates/simple/simple-editor";

import "./meeting-notes-editor.scss";

const DEBOUNCE_MS = 500;

interface MeetingNotesEditorProps {
  documentId: string;
  meetingId: string;
  initialBody: string | null;
  /** Keep parent meeting detail in sync after save so remounting (e.g. tab switch) hydrates fresh markdown. */
  onDocumentBodySaved?: (documentId: string, body: string) => void;
  /** Seconds since recording start to stamp new lines; null disables stamping. */
  stampElapsedSecs: number | null;
  /** Jump transcript to this meeting time (ms). */
  onSeekTranscript?: (timestampMs: number) => void;
}

export function MeetingNotesEditor({
  documentId,
  meetingId: _meetingId,
  initialBody,
  onDocumentBodySaved,
  stampElapsedSecs,
  onSeekTranscript,
}: MeetingNotesEditorProps) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastPersistedRef = useRef<string>(initialBody ?? "");
  const latestRef = useRef<string>(initialBody ?? "");
  const meetingNote = useMemo(
    () => ({ stampElapsedSecs }),
    [stampElapsedSecs],
  );

  const containerRef = useRef<HTMLDivElement>(null);

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
      className="meeting-notes-editor flex min-h-0 flex-1 flex-col"
    >
      <SimpleEditor
        key={documentId}
        initialMarkdown={initialBody}
        onMarkdownChange={onMarkdownChange}
        hideThemeToggle
        meetingNote={meetingNote}
      />
    </div>
  );
}
