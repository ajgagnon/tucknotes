import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { MeetingDetail } from "./types";

/** Sentinel value for the Transcript tab (not a `MeetingDocument` id). */
export const TRANSCRIPT_TAB = "__transcript__";
/** Sentinel for the Minutes tab while recording, before the minutes document
 *  exists (the backend creates it on the first LLM pass). */
export const MINUTES_TAB = "__minutes__";

/** Tab display order by document kind (Transcript always renders last).
 *  During recording the summary is hidden, so this yields Minutes → Notes. */
const KIND_TAB_ORDER: Record<string, number> = {
  summary: 0,
  minutes: 1,
  notes: 2,
};

/**
 * The meeting tab state machine: which document tabs are visible, which tab
 * is effectively selected (sentinels resolve to real documents as they
 * appear), and how auto-selection during a live recording backs off once the
 * user picks a tab manually.
 */
export function useMeetingTabs({
  meetingId,
  documents,
  isLiveRecording,
  minutesDocId,
  minutesExpected,
}: {
  meetingId: string;
  documents: MeetingDetail["documents"];
  isLiveRecording: boolean;
  minutesDocId: string | undefined;
  minutesExpected: boolean;
}) {
  const lastNonTranscriptTabRef = useRef<string>("");
  // Once the user explicitly picks a tab, auto-selection (e.g. the Minutes
  // default while recording) backs off until the next recording session.
  const userPickedTabRef = useRef(false);
  const wasLiveRecordingRef = useRef(false);
  const docIds = useMemo(
    () => documents.map((d) => d.id).join(","),
    [documents],
  );
  const [selectedDocId, setSelectedDocId] = useState("");

  const summaryHidden = isLiveRecording;
  const visibleDocuments = useMemo(() => {
    const docs = summaryHidden
      ? documents.filter((d) => d.kind !== "summary")
      : documents;
    return [...docs].sort(
      (a, b) =>
        (KIND_TAB_ORDER[a.kind] ?? 9) - (KIND_TAB_ORDER[b.kind] ?? 9) ||
        a.sort_order - b.sort_order,
    );
  }, [documents, summaryHidden]);

  const hasMinutesDoc = minutesDocId != null;
  const showSyntheticMinutesTab =
    isLiveRecording && minutesExpected && !hasMinutesDoc;

  const defaultDocumentTabId = useMemo(
    () =>
      visibleDocuments.find((d) => d.kind === "summary")?.id ??
      visibleDocuments[0]?.id ??
      "",
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [docIds, visibleDocuments],
  );

  const effectiveTabId = useMemo(() => {
    if (selectedDocId === TRANSCRIPT_TAB) return TRANSCRIPT_TAB;
    if (selectedDocId === MINUTES_TAB) {
      // Hands off to the real document once the first pass creates it.
      if (minutesDocId) return minutesDocId;
      if (showSyntheticMinutesTab) return MINUTES_TAB;
    } else if (visibleDocuments.some((d) => d.id === selectedDocId)) {
      return selectedDocId;
    }
    if (defaultDocumentTabId) return defaultDocumentTabId;
    return TRANSCRIPT_TAB;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    selectedDocId,
    defaultDocumentTabId,
    docIds,
    visibleDocuments,
    showSyntheticMinutesTab,
    minutesDocId,
  ]);

  const isTranscriptTab = effectiveTabId === TRANSCRIPT_TAB;
  const isSyntheticMinutesTab = effectiveTabId === MINUTES_TAB;

  const leaveTranscriptTab = useCallback(() => {
    setSelectedDocId((prev) => {
      if (prev !== TRANSCRIPT_TAB) return prev;
      return lastNonTranscriptTabRef.current || defaultDocumentTabId;
    });
  }, [defaultDocumentTabId]);

  const handleTabValueChange = useCallback(
    (v: string) => {
      userPickedTabRef.current = true;
      if (v === TRANSCRIPT_TAB) {
        if (effectiveTabId !== TRANSCRIPT_TAB) {
          lastNonTranscriptTabRef.current = effectiveTabId;
        }
      } else {
        lastNonTranscriptTabRef.current = v;
      }
      setSelectedDocId(v);
    },
    [effectiveTabId],
  );

  /** Programmatic jump to the transcript tab (counts as a user pick). */
  const showTranscriptTab = useCallback(() => {
    userPickedTabRef.current = true;
    setSelectedDocId(TRANSCRIPT_TAB);
  }, []);

  useEffect(() => {
    setSelectedDocId((prev) => {
      if (prev === TRANSCRIPT_TAB) return prev;
      // The Minutes sentinel stays selected while it's still meaningful
      // (synthetic tab showing, or resolvable to the real document).
      if (prev === MINUTES_TAB && (showSyntheticMinutesTab || minutesDocId)) {
        return prev;
      }
      if (visibleDocuments.some((d) => d.id === prev)) return prev;
      if (isLiveRecording) {
        if (minutesDocId) return minutesDocId;
        if (showSyntheticMinutesTab) return MINUTES_TAB;
        return (
          visibleDocuments.find((d) => d.kind === "notes")?.id ??
          visibleDocuments[0]?.id ??
          ""
        );
      }
      return (
        visibleDocuments.find((d) => d.kind === "summary")?.id ??
        visibleDocuments[0]?.id ??
        ""
      );
    });
  }, [
    meetingId,
    docIds,
    isLiveRecording,
    visibleDocuments,
    showSyntheticMinutesTab,
    minutesDocId,
  ]);

  useEffect(() => {
    userPickedTabRef.current = false;
  }, [meetingId]);

  useEffect(() => {
    const sessionStarted = isLiveRecording && !wasLiveRecordingRef.current;
    if (sessionStarted) {
      userPickedTabRef.current = false;
    }
    if (isLiveRecording && !userPickedTabRef.current) {
      if (minutesExpected) {
        // Keep following the minutes tab as the async gates (setting + model
        // ready) resolve after mount — not just on the session-start edge.
        setSelectedDocId(minutesDocId ?? MINUTES_TAB);
      } else if (sessionStarted) {
        const notesId = documents.find((d) => d.kind === "notes")?.id;
        if (notesId) setSelectedDocId(notesId);
      }
    }
    wasLiveRecordingRef.current = isLiveRecording;
  }, [isLiveRecording, docIds, documents, minutesExpected, minutesDocId]);

  const selectedDoc =
    isTranscriptTab || isSyntheticMinutesTab
      ? undefined
      : (visibleDocuments.find((d) => d.id === effectiveTabId) ??
        visibleDocuments[0]);

  return {
    visibleDocuments,
    effectiveTabId,
    isTranscriptTab,
    isSyntheticMinutesTab,
    showSyntheticMinutesTab,
    selectedDoc,
    handleTabValueChange,
    leaveTranscriptTab,
    showTranscriptTab,
  };
}
