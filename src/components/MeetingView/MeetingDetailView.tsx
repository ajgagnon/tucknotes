import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Sparkles, Settings2, PlusIcon } from "lucide-react";
import ReactMarkdown from "react-markdown";
import { invoke } from "@tauri-apps/api/core";
import {
  useRecording,
  type TranscriptSegment,
  type AppError,
} from "@/hooks/useRecording";
import { Button } from "@/components/ui/button";
import {
  Sheet,
  SheetContent,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  type MeetingDetail,
  type MeetingDocument,
  type MeetingTitleInfo,
  minutesBodyFromDocuments,
} from "./types";
import { useMeetingSummarization } from "./useMeetingSummarization";
import { ThinkingBlock } from "./ThinkingBlock";
import { LiveTranscript } from "./LiveTranscript";
import { PersistedTranscript } from "./PersistedTranscript";
import { RecordingErrorBanner } from "./RecordingErrorBanner";
import { TranscriptFab } from "./TranscriptFab";
import { MeetingDocumentEditor } from "./MeetingDocumentEditor";
import { cn } from "@/lib/utils";

interface MeetingDetailViewProps {
  detail: MeetingDetail;
  isLiveRecording: boolean;
  liveSegments: TranscriptSegment[];
  provisional: Record<string, TranscriptSegment>;
  error: AppError | null;
  onTitleChange?: (info: MeetingTitleInfo) => void;
  /** After starting capture for an existing meeting (resume completed). */
  onRecordingStarted?: (meetingId: string) => void;
  /** Reload meeting detail from the backend (e.g. after adding a document). */
  onRefreshMeeting?: () => void | Promise<void>;
  /** Merge persisted document body into local meeting detail (notes autosave). */
  onMeetingDocumentBodyUpdated?: (documentId: string, body: string) => void;
}

export function MeetingDetailView({
  detail,
  isLiveRecording,
  liveSegments,
  provisional,
  error,
  onTitleChange,
  onRecordingStarted,
  onRefreshMeeting,
  onMeetingDocumentBodyUpdated,
}: MeetingDetailViewProps) {
  const transcriptEndRef = useRef<HTMLDivElement>(null);
  const wasLiveRecordingRef = useRef(false);
  const [transcriptOpen, setTranscriptOpen] = useState(false);
  const docIds = useMemo(
    () => detail.documents.map((d) => d.id).join(","),
    [detail.documents],
  );
  const [selectedDocId, setSelectedDocId] = useState("");

  const {
    recording,
    paused,
    transcriptFinalizingMeetingId,
    resumeRecording,
    stopRecording,
    startRecording,
    seedLiveTranscript,
  } = useRecording();
  const transcriptFinalizing =
    transcriptFinalizingMeetingId === detail.meeting.id;

  const capturingThisMeeting =
    isLiveRecording && recording && !paused;

  const minutesBodyStored = minutesBodyFromDocuments(detail.documents);

  const {
    summarizing,
    streamedSummary,
    thinkingText,
    summaryError,
    llmModelReady,
    currentSummary,
    handleSummarize,
  } = useMeetingSummarization(
    detail.meeting,
    minutesBodyStored,
    onTitleChange,
  );

  useEffect(() => {
    setSelectedDocId((prev) => {
      if (detail.documents.some((d) => d.id === prev)) return prev;
      if (isLiveRecording) {
        return (
          detail.documents.find((d) => d.kind === "notes")?.id ??
          detail.documents[0]?.id ??
          ""
        );
      }
      return (
        detail.documents.find((d) => d.kind === "minutes")?.id ??
        detail.documents[0]?.id ??
        ""
      );
    });
  }, [detail.meeting.id, docIds, isLiveRecording]);

  useEffect(() => {
    if (isLiveRecording && !wasLiveRecordingRef.current) {
      const notesId = detail.documents.find((d) => d.kind === "notes")?.id;
      if (notesId) setSelectedDocId(notesId);
    }
    wasLiveRecordingRef.current = isLiveRecording;
  }, [isLiveRecording, docIds, detail.documents]);

  const effectiveTabId =
    selectedDocId ||
    detail.documents.find((d) => d.kind === "minutes")?.id ||
    detail.documents[0]?.id ||
    "";

  const selectedDoc =
    detail.documents.find((d) => d.id === effectiveTabId) ??
    detail.documents[0];

  useEffect(() => {
    if (isLiveRecording && transcriptOpen) {
      transcriptEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [liveSegments, provisional, isLiveRecording, transcriptOpen]);

  const handleTranscriptOpenChange = useCallback((open: boolean) => {
    setTranscriptOpen(open);
  }, []);

  const handleFooterPrimaryAction = useCallback(async () => {
    try {
      if (isLiveRecording && recording) {
        await stopRecording();
        setTranscriptOpen(false);
        return;
      }
      if (isLiveRecording && paused) {
        await resumeRecording();
        setTranscriptOpen(false);
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
      setTranscriptOpen(false);
    } catch {
      /* error surfaced via context */
    }
  }, [
    detail.meeting.id,
    detail.segments,
    isLiveRecording,
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

  const handleAddDocument = useCallback(async () => {
    try {
      const doc = await invoke<MeetingDocument>("create_meeting_document", {
        meetingId: detail.meeting.id,
        title: null,
      });
      await onRefreshMeeting?.();
      setSelectedDocId(doc.id);
    } catch (e) {
      console.error("create_meeting_document:", e);
    }
  }, [detail.meeting.id, onRefreshMeeting]);

  const minutesPanel = summarizing ? (
    <div className="text-sm leading-relaxed p-5">
      {thinkingText && !streamedSummary && (
        <ThinkingBlock text={thinkingText} />
      )}
      {streamedSummary && (
        <div className="prose prose-sm dark:prose-invert max-w-none">
          <ReactMarkdown>{streamedSummary}</ReactMarkdown>
          <span className="inline-block w-1.5 h-4 bg-primary animate-pulse ml-0.5 align-text-bottom rounded-sm" />
        </div>
      )}
    </div>
  ) : currentSummary ? (
    <div className="prose prose-sm dark:prose-invert max-w-none p-5">
      <ReactMarkdown>{currentSummary}</ReactMarkdown>
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

  const isMinutesTab = selectedDoc?.kind === "minutes";
  const panelMode: "streaming" | "placeholder" | "editor" | null = !selectedDoc
    ? null
    : isMinutesTab && summarizing
      ? "streaming"
      : isMinutesTab && !currentSummary
        ? "placeholder"
        : "editor";
  const editorInitialBody = isMinutesTab
    ? currentSummary
    : selectedDoc?.body ?? null;

  const documentPanel = !selectedDoc
    ? null
    : panelMode === "editor"
      ? (
          <MeetingDocumentEditor
            key={selectedDoc.id}
            documentId={selectedDoc.id}
            initialBody={editorInitialBody}
            onDocumentBodySaved={onMeetingDocumentBodyUpdated}
          />
        )
      : minutesPanel;

  const showSummarizeAction =
    selectedDoc?.kind === "minutes" &&
    !summarizing &&
    llmModelReady &&
    !transcriptFinalizing;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {error && <RecordingErrorBanner error={error} />}

      <div className="p-3 border-b border-muted flex shrink-0 flex-wrap items-center justify-between gap-2">
        <div className="flex min-w-0 flex-1 items-center gap-1.5 text-sm font-medium">
          {detail.documents.length > 0 ? (
            <Tabs
              value={effectiveTabId}
              onValueChange={setSelectedDocId}
              className="min-w-0 flex flex-row flex-wrap items-center gap-1.5"
            >
              <TabsList
                variant="line"
                className="h-auto min-h-8 flex-wrap gap-1 bg-transparent p-0"
              >
                {detail.documents.map((doc) => (
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
                  </TabsTrigger>
                ))}
              </TabsList>
            </Tabs>
          ) : null}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="size-8 shrink-0 rounded-full p-0 text-muted-foreground"
            aria-label="Add document"
            onClick={() => void handleAddDocument()}
          >
            <PlusIcon className="size-3.5" />
          </Button>
          {summarizing && (
            <span className="inline-block size-1.5 shrink-0 animate-pulse rounded-full bg-muted-foreground" />
          )}
        </div>
        <div className="flex items-center gap-2">
          {transcriptFinalizing && (
            <span className="text-xs text-muted-foreground tabular-nums">
              Saving transcript…
            </span>
          )}
          <TranscriptFab
            className="shrink-0"
            open={transcriptOpen}
            onOpenChange={handleTranscriptOpenChange}
            capturing={capturingThisMeeting}
            onStopRecording={
              isLiveRecording ? handleFabStopRecording : undefined
            }
          />
        </div>
      </div>

      <div className="relative flex min-h-0 flex-1 flex-col">
        <div
          className={cn(
            "min-h-0 flex-1",
            panelMode === "editor"
              ? "flex min-h-0 flex-col overflow-hidden"
              : "overflow-y-auto",
          )}
        >
          {documentPanel}
        </div>

        {isMinutesTab && !summarizing && summaryError && (
          <p className="shrink-0 px-5 pb-2 text-xs text-red-500 dark:text-red-400">
            {summaryError}
          </p>
        )}

        {!isLiveRecording && showSummarizeAction && (
          <Button
            type="button"
            onClick={() => void handleSummarize()}
            variant="secondary"
            className="absolute bottom-1 right-0 z-10 rounded-full text-xs"
          >
            {currentSummary ? "Resummarize" : "Summarize"}
            <Sparkles className="size-3 shrink-0 text-muted-foreground" />
          </Button>
        )}
      </div>

      <Sheet open={transcriptOpen} modal={false} onOpenChange={handleTranscriptOpenChange}>
        <SheetContent
          side="right"
          showCloseButton
          className="flex bg-background flex-col gap-0 rounded-t-2xl p-0"
        >
          <SheetHeader className="shrink-0 border-b px-4 py-3">
            <SheetTitle>Transcript</SheetTitle>
          </SheetHeader>

          <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
            {isLiveRecording ? (
              <div className="flex flex-col gap-3">
                <LiveTranscript
                  segments={liveSegments}
                  provisional={provisional}
                  scrollRef={transcriptEndRef}
                />
              </div>
            ) : (
              <div className="flex flex-col gap-3">
                <PersistedTranscript segments={detail.segments} />
              </div>
            )}
          </div>
          <SheetFooter className="mt-0 shrink-0 flex-row items-center justify-between gap-3 border-t px-4 py-3 sm:flex-row">
            <div className="min-w-0 flex-1">
              {isLiveRecording && recording && (
                <button
                  type="button"
                  onClick={() => void handleFooterPrimaryAction()}
                  className="text-sm font-medium text-danger hover:underline"
                >
                  Stop recording
                </button>
              )}
              {isLiveRecording && paused && (
                <button
                  type="button"
                  onClick={() => void handleFooterPrimaryAction()}
                  className="text-sm font-medium text-success hover:underline"
                >
                  Resume
                </button>
              )}
              {!isLiveRecording && !(recording || paused) && (
                <button
                  type="button"
                  onClick={() => void handleFooterPrimaryAction()}
                  className="text-sm font-medium text-success hover:underline"
                >
                  Resume
                </button>
              )}
              {!isLiveRecording && (recording || paused) && (
                <p className="text-xs text-muted-foreground">
                  Another meeting is being recorded.
                </p>
              )}
            </div>
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
          </SheetFooter>
        </SheetContent>
      </Sheet>
    </div>
  );
}
