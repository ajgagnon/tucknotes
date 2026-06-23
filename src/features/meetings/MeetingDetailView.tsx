import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Sparkles, Settings2, ListRestart } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { invoke } from "@tauri-apps/api/core";
import { useRecording, type TranscriptSegment } from "@/features/recording";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { SETTINGS_SECTION_TEMPLATES } from "@/features/settings/TemplateSection";
import { useLlmDownloadProgress } from "@/features/models";
import {
  type MeetingDetail,
  type MeetingTitleInfo,
  type TranscriptScrollHandle,
  summaryBodyFromDocuments,
} from "./types";
import { useMeetingSummarization } from "./useMeetingSummarization";
import { useLiveMinutes } from "./useLiveMinutes";
import { ThinkingBlock } from "./ThinkingBlock";
import { LiveTranscript } from "./LiveTranscript";
import { PersistedTranscript } from "./PersistedTranscript";
import { TranscriptFab } from "./TranscriptFab";
import { MeetingDocumentEditor } from "./MeetingDocumentEditor";
import { StreamingSummaryToolbarPlaceholder } from "./StreamingSummaryToolbarPlaceholder";
import { TranscriptActionsMenu } from "./TranscriptActionsMenu";
import { type TranscriptLine } from "./exportTranscript";
import { cn } from "@/lib/utils";

/** Sentinel value for the Transcript tab (not a `MeetingDocument` id). */
const TRANSCRIPT_TAB = "__transcript__";
/** Sentinel for the Minutes tab while recording, before the minutes document
 *  exists (the backend creates it on the first LLM pass). */
const MINUTES_TAB = "__minutes__";

/** Tab display order by document kind (Transcript always renders last).
 *  During recording the summary is hidden, so this yields Minutes → Notes. */
const KIND_TAB_ORDER: Record<string, number> = {
  summary: 0,
  minutes: 1,
  notes: 2,
};
/// Sentinel value for the "Edit templates…" action in the template Select.
const EDIT_TEMPLATES_VALUE = "__edit_templates__";

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
  const wasLiveRecordingRef = useRef(false);
  const lastNonTranscriptTabRef = useRef<string>("");
  // Once the user explicitly picks a tab, auto-selection (e.g. the Minutes
  // default while recording) backs off until the next recording session.
  const userPickedTabRef = useRef(false);
  const docIds = useMemo(
    () => detail.documents.map((d) => d.id).join(","),
    [detail.documents],
  );
  const [selectedDocId, setSelectedDocId] = useState("");

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

  const summaryHidden = isLiveRecording;
  const visibleDocuments = useMemo(() => {
    const docs = summaryHidden
      ? detail.documents.filter((d) => d.kind !== "summary")
      : detail.documents;
    return [...docs].sort(
      (a, b) =>
        (KIND_TAB_ORDER[a.kind] ?? 9) - (KIND_TAB_ORDER[b.kind] ?? 9) ||
        a.sort_order - b.sort_order,
    );
  }, [detail.documents, summaryHidden]);

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

  const defaultDocumentTabId = useMemo(
    () =>
      visibleDocuments.find((d) => d.kind === "summary")?.id ??
      visibleDocuments[0]?.id ??
      "",
    [docIds, visibleDocuments],
  );

  const handleSeekTranscript = useCallback((timestampMs: number) => {
    userPickedTabRef.current = true;
    setSelectedDocId(TRANSCRIPT_TAB);
    requestAnimationFrame(() => {
      transcriptScrollRef.current?.scrollToTimeMs(timestampMs);
    });
  }, []);

  // Segments backing the transcript copy/export actions. During a live
  // recording the committed `liveSegments` are authoritative; otherwise the
  // persisted segments from the loaded meeting detail are used.
  const transcriptLines: TranscriptLine[] = isLiveRecording
    ? liveSegments
    : detail.segments;

  const summaryBodyStored = summaryBodyFromDocuments(detail.documents);

  const {
    summarizing,
    streamedSummary,
    thinkingText,
    llmModelReady,
    currentSummary,
    handleSummarize,
    templates,
    selectedTemplate,
    handleTemplateChange,
  } = useMeetingSummarization(detail.meeting, summaryBodyStored, onTitleChange);

  const llmDownload = useLlmDownloadProgress();

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
  const showSyntheticMinutesTab =
    isLiveRecording && minutesExpected && !hasMinutesDoc;

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
    detail.meeting.id,
    docIds,
    isLiveRecording,
    visibleDocuments,
    showSyntheticMinutesTab,
    minutesDocId,
  ]);

  useEffect(() => {
    userPickedTabRef.current = false;
  }, [detail.meeting.id]);

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
        const notesId = detail.documents.find((d) => d.kind === "notes")?.id;
        if (notesId) setSelectedDocId(notesId);
      }
    }
    wasLiveRecordingRef.current = isLiveRecording;
  }, [
    isLiveRecording,
    docIds,
    detail.documents,
    minutesExpected,
    minutesDocId,
  ]);

  const selectedDoc =
    isTranscriptTab || isSyntheticMinutesTab
      ? undefined
      : (visibleDocuments.find((d) => d.id === effectiveTabId) ??
        visibleDocuments[0]);

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
    transcriptFinalizing || (summarizing && !streamedSummary && !thinkingText);

  const summaryPanel = showSummarySkeleton ? (
    <div className="space-y-4 p-5" aria-busy="true" aria-live="polite">
      <Skeleton className="h-5 w-1/3" />
      <div className="space-y-2">
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-11/12" />
        <Skeleton className="h-4 w-4/5" />
      </div>
      <Skeleton className="h-5 w-1/4" />
      <div className="space-y-2">
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-5/6" />
      </div>
    </div>
  ) : summarizing ? (
    <div className="simple-editor-wrapper meeting-summary-prose">
      <StreamingSummaryToolbarPlaceholder />
      <div className="simple-editor-content">
        <div
          className="tiptap ProseMirror simple-editor"
          style={{ whiteSpace: "normal" }}
        >
          {thinkingText && !streamedSummary && (
            <ThinkingBlock text={thinkingText} />
          )}
          {streamedSummary && (
            <>
              <ReactMarkdown remarkPlugins={[remarkGfm]}>
                {streamedSummary}
              </ReactMarkdown>
              <span className="inline-block w-1.5 h-4 bg-primary animate-pulse ml-0.5 align-text-bottom rounded-sm" />
            </>
          )}
        </div>
      </div>
    </div>
  ) : currentSummary ? (
    <div className="simple-editor-wrapper meeting-summary-prose">
      <StreamingSummaryToolbarPlaceholder />
      <div className="simple-editor-content">
        <div
          className="tiptap ProseMirror simple-editor"
          style={{ whiteSpace: "normal" }}
        >
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {currentSummary}
          </ReactMarkdown>
        </div>
      </div>
    </div>
  ) : llmModelReady === true ? (
    <div className="flex flex-col items-start gap-3 p-5">
      <p className="text-sm text-neutral-400 italic">
        Generate an AI summary of this meeting.
      </p>
      <Button
        variant="outline"
        size="sm"
        onClick={() => void handleSummarize()}
      >
        <Sparkles className="size-2.5" />
        Summarize
      </Button>
    </div>
  ) : llmDownload ? (
    <div className="flex flex-col gap-2 p-5 text-sm text-neutral-400">
      <span className="italic">
        {llmDownload.done
          ? "Finishing model download…"
          : `Downloading summarization model… ${Math.round(llmDownload.percent)}%`}
      </span>
      <div className="h-1 w-full max-w-xs overflow-hidden rounded-full bg-muted">
        <div
          className="h-full rounded-full bg-primary transition-all duration-300 ease-out"
          style={{ width: `${llmDownload.percent}%` }}
        />
      </div>
    </div>
  ) : (
    <p className="text-sm text-neutral-400 italic p-5">
      Download a summarization model in Settings to enable AI summaries.
    </p>
  );

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
    <div className="simple-editor-wrapper">
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
    summaryPanel
  );

  const transcriptPanel = (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {isLiveRecording ? (
          <div className="flex flex-col gap-3">
            <LiveTranscript
              ref={transcriptScrollRef}
              segments={liveSegments}
              provisional={provisional}
              scrollRef={transcriptEndRef}
            />
          </div>
        ) : (
          <div className="flex flex-col gap-3">
            <PersistedTranscript
              ref={transcriptScrollRef}
              segments={detail.segments}
            />
          </div>
        )}
      </div>
      <div className="mt-0 shrink-0 flex flex-row items-center gap-2 border-t px-4 py-3">
        {!isLiveRecording && (recording || paused) && (
          <p className="text-xs text-muted-foreground">
            Another meeting is being recorded.
          </p>
        )}
        <div className="flex items-center gap-2">
          <TranscriptActionsMenu
            meeting={detail.meeting}
            segments={transcriptLines}
            className="shrink-0"
          />
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Open Sound settings (macOS)"
                  onClick={() => {
                    invoke("open_sound_settings").catch((e) =>
                      console.error("open_sound_settings:", e),
                    );
                  }}
                />
              }
            >
              <Settings2 className="size-4" />
            </TooltipTrigger>
            <TooltipContent>Sound settings</TooltipContent>
          </Tooltip>
        </div>
      </div>
    </div>
  );

  const showSummarizeAction =
    isSummaryTab && !summarizing && llmModelReady && !transcriptFinalizing;

  const mainPanel = isTranscriptTab
    ? transcriptPanel
    : isSyntheticMinutesTab
      ? liveMinutesPanel
      : documentPanel;

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
                <TabsTrigger
                  value={MINUTES_TAB}
                  className={cn(
                    "h-8 shrink-0 rounded-full border px-3 text-muted-foreground text-xs",
                    "border-muted data-active:border-muted data-active:text-foreground",
                    "data-active:bg-muted",
                    "group-data-[variant=line]/tabs-list:data-active:bg-muted",
                    "dark:group-data-[variant=line]/tabs-list:data-active:bg-muted",
                    "after:hidden",
                  )}
                >
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
                  className={cn(
                    "h-8 shrink-0 rounded-full border px-3 text-muted-foreground text-xs",
                    "border-muted data-active:border-muted data-active:text-foreground",
                    "data-active:bg-muted",
                    "group-data-[variant=line]/tabs-list:data-active:bg-muted",
                    "dark:group-data-[variant=line]/tabs-list:data-active:bg-muted",
                    "after:hidden",
                  )}
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
              <TabsTrigger
                value={TRANSCRIPT_TAB}
                className={cn(
                  "h-8 shrink-0 rounded-full border px-3 text-muted-foreground text-xs",
                  "border-muted data-active:border-muted data-active:text-foreground",
                  "data-active:bg-muted",
                  "group-data-[variant=line]/tabs-list:data-active:bg-muted",
                  "dark:group-data-[variant=line]/tabs-list:data-active:bg-muted",
                  "after:hidden",
                )}
              >
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
          <div className="mt-0 shrink-0 flex flex-row items-center gap-2 border-t px-4 py-3">
            {templates.length > 0 && (
              <div className="flex items-center gap-1.5">
                <Select
                  value={selectedTemplate}
                  onValueChange={(value) => {
                    if (value === EDIT_TEMPLATES_VALUE) {
                      onOpenSettings?.(SETTINGS_SECTION_TEMPLATES);
                      return;
                    }
                    void handleTemplateChange(value as string);
                  }}
                >
                  <SelectTrigger size="sm" aria-label="Summary template">
                    <SelectValue>
                      {(value: string | null) =>
                        templates.find((t) => t.id === value)?.name ?? "Recap"
                      }
                    </SelectValue>
                  </SelectTrigger>
                  <SelectContent className="min-w-52">
                    <SelectGroup>
                      <SelectLabel>Templates</SelectLabel>
                      {templates.map((t) => (
                        <SelectItem key={t.id} value={t.id}>
                          {t.name}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                    {onOpenSettings && (
                      <SelectGroup>
                        <SelectSeparator />
                        <SelectItem value={EDIT_TEMPLATES_VALUE}>
                          Edit templates…
                        </SelectItem>
                      </SelectGroup>
                    )}
                  </SelectContent>
                </Select>
              </div>
            )}

            <Tooltip>
              <TooltipTrigger
                render={
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={currentSummary ? "Resummarize" : "Summarize"}
                    onClick={() => void handleSummarize()}
                  />
                }
              >
                <ListRestart className="size-4" />
              </TooltipTrigger>
              <TooltipContent>
                {currentSummary ? "Resummarize" : "Summarize"}
              </TooltipContent>
            </Tooltip>
          </div>
        )}
      </div>
    </div>
  );
}
