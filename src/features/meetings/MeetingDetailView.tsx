import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Sparkles,
  Settings2,
  Play,
  ChevronDown,
  Pencil,
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { invoke } from "@tauri-apps/api/core";
import {
  useRecording,
  type TranscriptSegment,
  type AppError,
} from "@/features/recording";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { SETTINGS_SECTION_TEMPLATES } from "@/features/settings/TemplateSection";
import {
  type MeetingDetail,
  type MeetingTitleInfo,
  type TranscriptScrollHandle,
  summaryBodyFromDocuments,
} from "./types";
import { useMeetingSummarization } from "./useMeetingSummarization";
import { ThinkingBlock } from "./ThinkingBlock";
import { LiveTranscript } from "./LiveTranscript";
import { PersistedTranscript } from "./PersistedTranscript";
import { RecordingErrorBanner } from "./RecordingErrorBanner";
import { TranscriptFab } from "./TranscriptFab";
import { MeetingDocumentEditor } from "./MeetingDocumentEditor";
import { StreamingSummaryToolbarPlaceholder } from "./StreamingSummaryToolbarPlaceholder";
import { cn } from "@/lib/utils";

/** Sentinel value for the Transcript tab (not a `MeetingDocument` id). */
const TRANSCRIPT_TAB = "__transcript__";

interface MeetingDetailViewProps {
  detail: MeetingDetail;
  isLiveRecording: boolean;
  liveSegments: TranscriptSegment[];
  provisional: Record<string, TranscriptSegment>;
  error: AppError | null;
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
  error,
  onTitleChange,
  onRecordingStarted,
  onRefreshMeeting: _onRefreshMeeting,
  onMeetingDocumentBodyUpdated,
  onOpenSettings,
}: MeetingDetailViewProps) {
  const transcriptEndRef = useRef<HTMLDivElement>(null);
  const transcriptScrollRef = useRef<TranscriptScrollHandle | null>(null);
  const wasLiveRecordingRef = useRef(false);
  const lastNonTranscriptTabRef = useRef<string>("");
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
  const visibleDocuments = useMemo(
    () =>
      summaryHidden
        ? detail.documents.filter((d) => d.kind !== "summary")
        : detail.documents,
    [detail.documents, summaryHidden],
  );

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

  const effectiveTabId = useMemo(() => {
    if (selectedDocId === TRANSCRIPT_TAB) return TRANSCRIPT_TAB;
    if (visibleDocuments.some((d) => d.id === selectedDocId)) {
      return selectedDocId;
    }
    if (defaultDocumentTabId) return defaultDocumentTabId;
    return TRANSCRIPT_TAB;
  }, [selectedDocId, defaultDocumentTabId, docIds, visibleDocuments]);

  const isTranscriptTab = effectiveTabId === TRANSCRIPT_TAB;

  const handleSeekTranscript = useCallback((timestampMs: number) => {
    setSelectedDocId(TRANSCRIPT_TAB);
    requestAnimationFrame(() => {
      transcriptScrollRef.current?.scrollToTimeMs(timestampMs);
    });
  }, []);

  const summaryBodyStored = summaryBodyFromDocuments(detail.documents);

  const {
    summarizing,
    streamedSummary,
    thinkingText,
    summaryError,
    llmModelReady,
    currentSummary,
    handleSummarize,
    templates,
    selectedTemplate,
    handleTemplateChange,
  } = useMeetingSummarization(detail.meeting, summaryBodyStored, onTitleChange);

  const leaveTranscriptTab = useCallback(() => {
    setSelectedDocId((prev) => {
      if (prev !== TRANSCRIPT_TAB) return prev;
      return lastNonTranscriptTabRef.current || defaultDocumentTabId;
    });
  }, [defaultDocumentTabId]);

  const handleTabValueChange = useCallback(
    (v: string) => {
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
      if (visibleDocuments.some((d) => d.id === prev)) return prev;
      if (isLiveRecording) {
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
  }, [detail.meeting.id, docIds, isLiveRecording, visibleDocuments]);

  useEffect(() => {
    if (isLiveRecording && !wasLiveRecordingRef.current) {
      const notesId = detail.documents.find((d) => d.kind === "notes")?.id;
      if (notesId) setSelectedDocId(notesId);
    }
    wasLiveRecordingRef.current = isLiveRecording;
  }, [isLiveRecording, docIds, detail.documents]);

  const selectedDoc = isTranscriptTab
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
  ) : llmModelReady === false ? (
    <p className="text-sm text-neutral-400 italic p-5">
      Download a summarization model in Settings to enable AI summaries.
    </p>
  ) : (
    <p className="text-sm text-neutral-400 italic p-5">
      Click &ldquo;Summarize&rdquo; to generate an AI summary.
    </p>
  );

  const isSummaryTab = selectedDoc?.kind === "summary";
  const panelMode: "streaming" | "placeholder" | "editor" | null = !selectedDoc
    ? null
    : isSummaryTab && summarizing
      ? "streaming"
      : isSummaryTab && !currentSummary
        ? "placeholder"
        : "editor";
  const editorInitialBody = isSummaryTab
    ? currentSummary
    : (selectedDoc?.body ?? null);

  const documentPanel = !selectedDoc ? null : panelMode === "editor" ? (
    <MeetingDocumentEditor
      key={selectedDoc.id}
      documentId={selectedDoc.id}
      initialBody={editorInitialBody}
      onDocumentBodySaved={onMeetingDocumentBodyUpdated}
      stampElapsedSecs={isSummaryTab ? null : stampElapsedSecs}
      onSeekTranscript={handleSeekTranscript}
      className={isSummaryTab ? "meeting-summary-prose" : undefined}
    />
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
        {isLiveRecording && recording && (
          <button
            type="button"
            onClick={() => void handleFooterPrimaryAction()}
            className="text-sm font-medium text-danger hover:underline"
          >
            Stop recording
          </button>
        )}
        {canResume && (
          <button
            type="button"
            onClick={() => void handleFooterPrimaryAction()}
            className="inline-flex items-center gap-1.5 text-sm font-medium text-muted-foreground hover:underline"
          >
            <Play className="size-3 shrink-0" />
            Resume
          </button>
        )}
        {!isLiveRecording && (recording || paused) && (
          <p className="text-xs text-muted-foreground">
            Another meeting is being recorded.
          </p>
        )}
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          className="shrink-0 text-muted-foreground hover:text-foreground"
          aria-label="Open Sound settings (macOS)"
          title="Sound settings"
          onClick={() => {
            invoke("open_sound_settings").catch((e) =>
              console.error("open_sound_settings:", e),
            );
          }}
        >
          <Settings2 className="size-4" />
        </Button>
      </div>
    </div>
  );

  const showSummarizeAction =
    isSummaryTab && !summarizing && llmModelReady && !transcriptFinalizing;

  const mainPanel = isTranscriptTab ? transcriptPanel : documentPanel;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {error && <RecordingErrorBanner error={error} />}

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
            <ButtonGroup className="shrink-0">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => void handleSummarize()}
              >
                <Sparkles />
                {currentSummary ? "Resummarize" : "Summarize"}
              </Button>

              {templates.length > 0 && (
                <DropdownMenu>
                  <DropdownMenuTrigger
                    render={
                      <Button
                        variant="outline"
                        size="sm"
                        aria-label="Summary template"
                      />
                    }
                  >
                    {templates.find((t) => t.id === selectedTemplate)?.name ??
                      "Default"}
                    <ChevronDown className="text-muted-foreground" />
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="start" className="min-w-52">
                    <DropdownMenuRadioGroup
                      value={selectedTemplate}
                      onValueChange={(value) =>
                        void handleTemplateChange(value as string)
                      }
                    >
                      {templates.map((t) => (
                        <DropdownMenuRadioItem key={t.id} value={t.id}>
                          {t.name}
                        </DropdownMenuRadioItem>
                      ))}
                    </DropdownMenuRadioGroup>
                    {onOpenSettings && (
                      <>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem
                          onClick={() =>
                            onOpenSettings(SETTINGS_SECTION_TEMPLATES)
                          }
                        >
                          <Pencil className="size-2.5" />
                          Edit templates…
                        </DropdownMenuItem>
                      </>
                    )}
                  </DropdownMenuContent>
                </DropdownMenu>
              )}
            </ButtonGroup>
            {summaryError && (
              <p className="min-w-0 flex-1 truncate text-xs text-red-500 dark:text-red-400">
                {summaryError}
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
