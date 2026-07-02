import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { invoke } from "@tauri-apps/api/core";
import { useRecording, type TranscriptSegment } from "@/features/recording";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  type MeetingDetail,
  type MeetingTitleInfo,
  type TranscriptScrollHandle,
  summaryBodyFromDocuments,
} from "./types";
import { useMeetingSummarization } from "./useMeetingSummarization";
import { useLiveMinutes } from "./useLiveMinutes";
import {
  useMeetingTabs,
  TRANSCRIPT_TAB,
  MINUTES_TAB,
} from "./useMeetingTabs";
import { SummaryPanel } from "./SummaryPanel";
import { SummarizeFooter } from "./SummarizeFooter";
import { TranscriptPanel } from "./TranscriptPanel";
import { TranscriptFab } from "./TranscriptFab";
import { MeetingDocumentEditor } from "./MeetingDocumentEditor";
import { StreamingSummaryToolbarPlaceholder } from "./StreamingSummaryToolbarPlaceholder";
import { cn } from "@/lib/utils";

const tabTriggerClass = cn(
  "h-8 shrink-0 rounded-full border px-3 text-muted-foreground text-xs",
  "border-muted data-active:border-muted data-active:text-foreground",
  "data-active:bg-muted",
  "group-data-[variant=line]/tabs-list:data-active:bg-muted",
  "dark:group-data-[variant=line]/tabs-list:data-active:bg-muted",
  "after:hidden",
);

interface MeetingDetailViewProps {
  detail: MeetingDetail;
  isLiveRecording: boolean;
  liveSegments: TranscriptSegment[];
  provisional: Record<string, TranscriptSegment>;
  onTitleChange?: (info: MeetingTitleInfo) => void;
  /** After starting capture for an existing meeting (resume completed). */
  onRecordingStarted?: (meetingId: string) => void;
  /** Reload meeting detail from the backend. */
  onRefreshMeeting?: () => void | Promise<void>;
  /** Merge persisted document body into local meeting detail (notes autosave). */
  onMeetingDocumentBodyUpdated?: (documentId: string, body: string) => void;
  /** Switch the app to the settings view (e.g. to change the LLM model).
   *  Pass a section id to deep-link/scroll to a specific settings section. */
  onOpenSettings?: (section?: string) => void;
}

export function MeetingDetailView({
  detail,
  isLiveRecording,
  liveSegments,
  provisional,
  onTitleChange,
  onRecordingStarted,
  onRefreshMeeting,
  onMeetingDocumentBodyUpdated,
  onOpenSettings,
}: MeetingDetailViewProps) {
  const transcriptEndRef = useRef<HTMLDivElement>(null);
  const liveMinutesEndRef = useRef<HTMLDivElement>(null);
  const transcriptScrollRef = useRef<TranscriptScrollHandle | null>(null);

  const {
    recording,
    paused,
    meetingId: recordingMeetingId,
    elapsed,
    transcriptFinalizingMeetingId,
    resumeRecording,
    stopRecording,
    startRecording,
    seedLiveTranscript,
  } = useRecording();
  const transcriptFinalizing =
    transcriptFinalizingMeetingId === detail.meeting.id;

  const capturingThisMeeting = isLiveRecording && recording && !paused;
  const canResume =
    (isLiveRecording && paused) || (!isLiveRecording && !(recording || paused));

  const stampElapsedSecs =
    isLiveRecording &&
    recordingMeetingId === detail.meeting.id &&
    (recording || paused) &&
    elapsed > 0
      ? elapsed
      : null;

  const summaryBodyStored = summaryBodyFromDocuments(detail.documents);

  const {
    summarizing,
    sections,
    llmModelReady,
    currentSummary,
    handleSummarize,
    templates,
    selectedTemplate,
    handleTemplateChange,
  } = useMeetingSummarization(detail.meeting, summaryBodyStored, onTitleChange);

  const minutesDocId = useMemo(
    () => detail.documents.find((d) => d.kind === "minutes")?.id,
    [detail.documents],
  );
  const hasMinutesDoc = minutesDocId != null;
  // Bumped on every external minutes update (LLM pass) so the post-meeting
  // editor remounts with the fresh body — it only reads initialBody at mount.
  // User edits save through onDocumentBodySaved and never bump this.
  const [minutesRev, setMinutesRev] = useState(0);
  const minutesDocIdRef = useRef(minutesDocId);
  minutesDocIdRef.current = minutesDocId;
  const onMeetingDocumentBodyUpdatedRef = useRef(onMeetingDocumentBodyUpdated);
  onMeetingDocumentBodyUpdatedRef.current = onMeetingDocumentBodyUpdated;
  const handleMinutesBody = useCallback((body: string) => {
    const docId = minutesDocIdRef.current;
    if (docId == null) return;
    onMeetingDocumentBodyUpdatedRef.current?.(docId, body);
    setMinutesRev((rev) => rev + 1);
  }, []);
  const { liveMinutesBody } = useLiveMinutes(
    detail.meeting.id,
    isLiveRecording,
    hasMinutesDoc,
    onRefreshMeeting,
    handleMinutesBody,
  );

  // Mirrors the backend's session gating (setting on + model downloaded) so
  // the Minutes tab can appear immediately — before the first LLM pass
  // creates the document — without promising minutes that will never come.
  const [liveMinutesEnabled, setLiveMinutesEnabled] = useState(false);
  useEffect(() => {
    invoke<boolean>("get_live_minutes_enabled")
      .then(setLiveMinutesEnabled)
      .catch(() => setLiveMinutesEnabled(false));
  }, []);
  const minutesExpected = liveMinutesEnabled && llmModelReady === true;

  const {
    visibleDocuments,
    effectiveTabId,
    isTranscriptTab,
    isSyntheticMinutesTab,
    showSyntheticMinutesTab,
    selectedDoc,
    handleTabValueChange,
    leaveTranscriptTab,
    showTranscriptTab,
  } = useMeetingTabs({
    meetingId: detail.meeting.id,
    documents: detail.documents,
    isLiveRecording,
    minutesDocId,
    minutesExpected,
  });

  const handleSeekTranscript = useCallback(
    (timestampMs: number) => {
      showTranscriptTab();
      requestAnimationFrame(() => {
        transcriptScrollRef.current?.scrollToTimeMs(timestampMs);
      });
    },
    [showTranscriptTab],
  );

  useEffect(() => {
    if (isLiveRecording && isTranscriptTab) {
      transcriptEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [liveSegments, provisional, isLiveRecording, isTranscriptTab]);

  const handleFooterPrimaryAction = useCallback(async () => {
    try {
      if (isLiveRecording && recording) {
        await stopRecording();
        leaveTranscriptTab();
        return;
      }
      if (isLiveRecording && paused) {
        await resumeRecording();
        leaveTranscriptTab();
        return;
      }
      if (!isLiveRecording && (recording || paused)) {
        return;
      }
      const id = await startRecording(detail.meeting.id);
      onRecordingStarted?.(id);
      seedLiveTranscript(
        detail.segments.map((s) => ({
          text: s.text,
          source: s.source,
          timestamp_ms: s.timestamp_ms,
          is_provisional: false,
        })),
      );
      leaveTranscriptTab();
    } catch {
      /* error surfaced via context */
    }
  }, [
    detail.meeting.id,
    detail.segments,
    isLiveRecording,
    leaveTranscriptTab,
    onRecordingStarted,
    paused,
    recording,
    resumeRecording,
    seedLiveTranscript,
    startRecording,
    stopRecording,
  ]);

  const handleFabStopRecording = useCallback(async () => {
    try {
      await stopRecording();
    } catch {
      /* error surfaced via context */
    }
  }, [stopRecording]);

  const handleFabResume = useCallback(() => {
    void handleFooterPrimaryAction();
  }, [handleFooterPrimaryAction]);

  const showSummarySkeleton =
    transcriptFinalizing || (summarizing && sections.length === 0);

  const isSummaryTab = selectedDoc?.kind === "summary";
  // During recording the LLM owns the minutes document (each pass replaces
  // the whole body), so it renders read-only; editable once recording ends.
  const isLiveMinutesTab = selectedDoc?.kind === "minutes" && isLiveRecording;
  const panelMode:
    | "streaming"
    | "placeholder"
    | "editor"
    | "live-minutes"
    | null = !selectedDoc
    ? null
    : isLiveMinutesTab
      ? "live-minutes"
      : isSummaryTab && summarizing
        ? "streaming"
        : isSummaryTab && !currentSummary
          ? "placeholder"
          : "editor";
  const editorInitialBody = isSummaryTab
    ? currentSummary
    : (selectedDoc?.body ?? null);

  const liveMinutesContent = liveMinutesBody ?? selectedDoc?.body ?? "";
  // Plain editor shell (no `meeting-summary-prose`): the live view inherits
  // the editor's native list styling so it matches the post-recording editor,
  // instead of the summary's flush-left em-dash markers.
  const liveMinutesPanel = liveMinutesContent ? (
    // Reuse the editor shell for styling only, not as its own scroll area: the
    // surrounding tab panel already scrolls, and the shell's height:100% plus the
    // sticky toolbar would otherwise overflow by the toolbar height, leaving the
    // panel permanently scrollable with almost no text.
    <div
      className="simple-editor-wrapper"
      style={{ height: "auto", maxHeight: "none", overflow: "visible" }}
    >
      <StreamingSummaryToolbarPlaceholder />
      <div className="simple-editor-content">
        <div
          className="tiptap ProseMirror simple-editor"
          style={{ whiteSpace: "normal" }}
        >
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {liveMinutesContent}
          </ReactMarkdown>
        </div>
        <div ref={liveMinutesEndRef} />
      </div>
    </div>
  ) : (
    <p className="text-sm text-neutral-400 italic p-5">
      Minutes will appear here as the meeting progresses.
    </p>
  );

  useEffect(() => {
    if (isLiveRecording && (isLiveMinutesTab || isSyntheticMinutesTab)) {
      liveMinutesEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [
    liveMinutesContent,
    isLiveRecording,
    isLiveMinutesTab,
    isSyntheticMinutesTab,
  ]);

  const documentPanel = !selectedDoc ? null : panelMode === "editor" ? (
    <MeetingDocumentEditor
      key={
        selectedDoc.kind === "minutes"
          ? `${selectedDoc.id}:${minutesRev}`
          : selectedDoc.id
      }
      documentId={selectedDoc.id}
      initialBody={editorInitialBody}
      onDocumentBodySaved={onMeetingDocumentBodyUpdated}
      stampElapsedSecs={isSummaryTab ? null : stampElapsedSecs}
      onSeekTranscript={handleSeekTranscript}
      className={isSummaryTab ? "meeting-summary-prose" : undefined}
      summaryActions={isSummaryTab}
    />
  ) : panelMode === "live-minutes" ? (
    liveMinutesPanel
  ) : (
    <SummaryPanel
      showSkeleton={showSummarySkeleton}
      summarizing={summarizing}
      sections={sections}
      currentSummary={currentSummary}
      llmModelReady={llmModelReady}
      onSummarize={() => void handleSummarize()}
    />
  );

  const showSummarizeAction =
    isSummaryTab && !summarizing && llmModelReady && !transcriptFinalizing;

  const mainPanel = isTranscriptTab ? (
    <TranscriptPanel
      meeting={detail.meeting}
      persistedSegments={detail.segments}
      isLiveRecording={isLiveRecording}
      scrollHandleRef={transcriptScrollRef}
      endRef={transcriptEndRef}
    />
  ) : isSyntheticMinutesTab ? (
    liveMinutesPanel
  ) : (
    documentPanel
  );

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="p-3 border-b border-muted flex shrink-0 flex-wrap items-center justify-between gap-2">
        <div className="flex min-w-0 flex-1 items-center gap-1.5 text-sm font-medium">
          <Tabs
            value={effectiveTabId}
            onValueChange={handleTabValueChange}
            className="min-w-0 flex flex-row flex-wrap items-center gap-1.5"
          >
            <TabsList
              variant="line"
              className="h-auto min-h-8 flex-wrap gap-1 bg-transparent p-0"
            >
              {showSyntheticMinutesTab && (
                <TabsTrigger value={MINUTES_TAB} className={tabTriggerClass}>
                  Minutes
                  <span
                    className="inline-block size-1.5 shrink-0 animate-pulse rounded-full bg-danger"
                    aria-label="Live"
                  />
                </TabsTrigger>
              )}
              {visibleDocuments.map((doc) => (
                <TabsTrigger
                  key={doc.id}
                  value={doc.id}
                  className={tabTriggerClass}
                >
                  {doc.title}
                  {summarizing && doc.kind === "summary" && (
                    <span className="inline-block size-1.5 shrink-0 animate-pulse rounded-full bg-muted-foreground" />
                  )}
                  {isLiveRecording && doc.kind === "minutes" && (
                    <span
                      className="inline-block size-1.5 shrink-0 animate-pulse rounded-full bg-destructive"
                      aria-label="Live"
                    />
                  )}
                </TabsTrigger>
              ))}
              <TabsTrigger value={TRANSCRIPT_TAB} className={tabTriggerClass}>
                Transcript
              </TabsTrigger>
            </TabsList>
          </Tabs>
        </div>
        <div className="flex items-center gap-2">
          {transcriptFinalizing && (
            <span className="text-xs text-muted-foreground tabular-nums">
              Saving transcript…
            </span>
          )}
          <TranscriptFab
            className="shrink-0"
            capturing={capturingThisMeeting}
            onResume={canResume ? handleFabResume : undefined}
            onStopRecording={
              capturingThisMeeting ? handleFabStopRecording : undefined
            }
          />
        </div>
      </div>

      <div className="relative flex min-h-0 flex-1 flex-col">
        <div
          className={cn(
            "min-h-0 flex-1",
            isTranscriptTab
              ? "flex min-h-0 flex-col"
              : panelMode === "editor"
                ? "flex min-h-0 flex-col overflow-hidden"
                : "overflow-y-auto",
          )}
        >
          {mainPanel}
        </div>

        {!isLiveRecording && showSummarizeAction && (
          <SummarizeFooter
            templates={templates}
            selectedTemplate={selectedTemplate}
            onTemplateChange={(id) => void handleTemplateChange(id)}
            onSummarize={() => void handleSummarize()}
            hasSummary={currentSummary != null}
            onOpenSettings={onOpenSettings}
          />
        )}
      </div>
    </div>
  );
}
