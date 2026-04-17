import { useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SimpleEditor } from "@/components/tiptap-templates/simple/simple-editor";

const DEBOUNCE_MS = 500;

interface MeetingDocumentEditorProps {
  documentId: string;
  initialBody: string | null;
  /** Keep parent meeting detail in sync after save so remounting (e.g. tab switch) hydrates fresh markdown. */
  onDocumentBodySaved?: (documentId: string, body: string) => void;
}

export function MeetingDocumentEditor({
  documentId,
  initialBody,
  onDocumentBodySaved,
}: MeetingDocumentEditorProps) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastPersistedRef = useRef<string>(initialBody ?? "");
  const latestRef = useRef<string>(initialBody ?? "");

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
    <div className="meeting-document-editor flex min-h-0 flex-1 flex-col">
      <SimpleEditor
        key={documentId}
        initialMarkdown={initialBody}
        onMarkdownChange={onMarkdownChange}
        hideThemeToggle
      />
    </div>
  );
}
