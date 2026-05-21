import { useCallback, useEffect, useRef, useState } from "react";
import {
  FileText,
  Fullscreen,
  Loader2,
  MessageCircle,
  Search,
  X,
} from "lucide-react";
import type { ChatStatus } from "ai";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Conversation,
  ConversationContent,
  ConversationEmptyState,
  ConversationScrollButton,
} from "@/components/ai-elements/conversation";
import { Message, MessageContent } from "@/components/ai-elements/message";
import {
  PromptInput,
  PromptInputBody,
  PromptInputButton,
  PromptInputFooter,
  PromptInputSubmit,
  PromptInputTextarea,
  PromptInputTools,
  type PromptInputMessage,
} from "@/components/ai-elements/prompt-input";
import { cn } from "@/lib/utils";
import { Streamdown } from "streamdown";

import { useChatStream, type SearchHit } from "./use-chat-stream";
import { CitationChip } from "./CitationChip";
import { rehypeCitations } from "./rehype-citations";

type ToolCall = {
  id: string;
  name: string;
  args: string;
  status: "running" | "done";
  hits?: SearchHit[];
};

type ChatMessage = {
  id: string;
  role: "user" | "assistant";
  text: string;
  toolCalls?: ToolCall[];
};

const HISTORY_CAP = 12;

const newId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : Math.random().toString(36).slice(2);

type ChatbotProps = {
  activeMeeting?: { id: string; title: string | null } | null;
  onOpenSettings?: () => void;
  onOpenMeeting?: (meetingId: string) => void;
};

export function Chatbot({
  activeMeeting,
  onOpenSettings,
  onOpenMeeting,
}: ChatbotProps = {}) {
  const [open, setOpen] = useState(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [status, setStatus] = useState<ChatStatus | undefined>(undefined);
  const [errorText, setErrorText] = useState<string | null>(null);
  const [dismissedContext, setDismissedContext] = useState(false);
  const [confirmCloseOpen, setConfirmCloseOpen] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const { send, stop, modelReady } = useChatStream();

  const requestClose = useCallback(() => {
    if (messages.length > 0) {
      setConfirmCloseOpen(true);
      return;
    }
    setOpen(false);
  }, [messages.length]);

  useEffect(() => {
    if (!open) {
      // Per-panel-open session: clear history + status on close.
      setMessages([]);
      setStatus(undefined);
      setErrorText(null);
      setConfirmCloseOpen(false);
      return;
    }
    setDismissedContext(false);
    const raf = requestAnimationFrame(() => textareaRef.current?.focus());
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        // Don't close the panel via Escape if the confirmation dialog is up —
        // Escape should dismiss the dialog instead.
        if (confirmCloseOpen) return;
        requestClose();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      cancelAnimationFrame(raf);
      document.removeEventListener("keydown", onKey);
    };
  }, [open, confirmCloseOpen, requestClose]);

  const showContextChip = open && !!activeMeeting && !dismissedContext;
  const canSubmit = modelReady === true;

  const appendChunk = useCallback((id: string, chunk: string) => {
    setMessages((prev) =>
      prev.map((m) => (m.id === id ? { ...m, text: m.text + chunk } : m)),
    );
  }, []);

  const upsertToolCall = useCallback(
    (assistantId: string, callId: string, patch: Partial<ToolCall>) => {
      setMessages((prev) =>
        prev.map((m) => {
          if (m.id !== assistantId) return m;
          const existing = m.toolCalls ?? [];
          const idx = existing.findIndex((c) => c.id === callId);
          if (idx === -1) {
            const next: ToolCall = {
              id: callId,
              name: patch.name ?? "",
              args: patch.args ?? "",
              status: patch.status ?? "running",
              hits: patch.hits,
            };
            return { ...m, toolCalls: [...existing, next] };
          }
          const updated = [...existing];
          updated[idx] = { ...updated[idx], ...patch };
          return { ...m, toolCalls: updated };
        }),
      );
    },
    [],
  );

  const handleSubmit = useCallback(
    async (message: PromptInputMessage) => {
      const text = message.text?.trim();
      if (!text || status === "submitted" || status === "streaming") return;
      if (!canSubmit) return;

      setErrorText(null);

      const userMsg: ChatMessage = { id: newId(), role: "user", text };
      const assistantMsg: ChatMessage = {
        id: newId(),
        role: "assistant",
        text: "",
      };
      setMessages((prev) => [...prev, userMsg, assistantMsg]);
      requestAnimationFrame(() => textareaRef.current?.focus());

      setStatus("submitted");
      const meetingId =
        showContextChip && activeMeeting ? activeMeeting.id : null;
      // Build history from the messages we just appended; cap to last N entries.
      const fullHistory = [
        ...messages.map((m) => ({ role: m.role, text: m.text })),
        { role: "user" as const, text },
      ];
      const history = fullHistory.slice(-HISTORY_CAP);

      let firstToken = true;
      await send({
        meetingId,
        history,
        onToken: (chunk) => {
          if (firstToken) {
            firstToken = false;
            setStatus("streaming");
          }
          appendChunk(assistantMsg.id, chunk);
        },
        onComplete: () => {
          setStatus(undefined);
        },
        onError: (msg) => {
          setStatus("error");
          setErrorText(msg);
        },
        onToolCallStart: (callId, name) => {
          setStatus("streaming");
          upsertToolCall(assistantMsg.id, callId, {
            name,
            status: "running",
            args: "",
          });
        },
        onToolCallArgsDelta: (callId, delta) => {
          upsertToolCall(assistantMsg.id, callId, {});
          setMessages((prev) =>
            prev.map((m) => {
              if (m.id !== assistantMsg.id || !m.toolCalls) return m;
              return {
                ...m,
                toolCalls: m.toolCalls.map((c) =>
                  c.id === callId ? { ...c, args: c.args + delta } : c,
                ),
              };
            }),
          );
        },
        onToolResult: (callId, _name, hits) => {
          upsertToolCall(assistantMsg.id, callId, {
            status: "done",
            hits,
          });
        },
      });
    },
    [
      status,
      send,
      appendChunk,
      upsertToolCall,
      showContextChip,
      activeMeeting,
      messages,
      canSubmit,
    ],
  );

  const handleStop = useCallback(() => {
    stop();
    setStatus(undefined);
  }, [stop]);

  return (
    <>
      {!open && (
        <Button
          type="button"
          onClick={() => setOpen(true)}
          aria-label="Ask Tuck"
          className="fixed bottom-4 right-4 z-50 h-11 gap-2 rounded-full px-4 shadow-lg"
        >
          <MessageCircle className="size-4" />
          <span className="text-sm font-medium">Ask Tuck</span>
        </Button>
      )}

      {open && (
        <div
          role="dialog"
          aria-label="Chat"
          className={cn(
            "fixed bottom-4 right-4 z-50 flex h-[560px] w-[380px] flex-col overflow-hidden rounded-xl border border-border bg-[color-mix(in_oklab,var(--background),var(--popover)_45%)] text-popover-foreground shadow-xl",
            "animate-in fade-in zoom-in-95 slide-in-from-bottom-2 duration-200",
          )}
        >
          <header className="flex h-11 shrink-0 items-center justify-between border-b border-border px-3">
            <span className="text-sm font-medium">Chat</span>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              onClick={requestClose}
              aria-label="Close chat"
            >
              <X className="size-4" />
            </Button>
          </header>

          <Conversation className="flex-1">
            <ConversationContent className="gap-4 p-3">
              {messages.length === 0 && (
                <>
                  <ConversationEmptyState
                    icon={<MessageCircle className="size-8" />}
                    title={
                      modelReady === false
                        ? "Model required"
                        : "Ask me anything"
                    }
                    description={
                      modelReady === false
                        ? "Download a model in Settings to use Tuck."
                        : "I'm here to help you get more out of your meetings."
                    }
                  />
                  {modelReady === false && onOpenSettings && (
                    <div className="-mt-4 flex justify-center">
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={onOpenSettings}
                      >
                        Open Settings
                      </Button>
                    </div>
                  )}
                </>
              )}
              {messages.map((m) => {
                const sources: SearchHit[] =
                  m.toolCalls?.flatMap((tc) => tc.hits ?? []) ?? [];
                return (
                  <Message from={m.role} key={m.id}>
                    <MessageContent>
                      {m.role === "assistant" ? (
                        <>
                          {m.toolCalls?.map((tc) => (
                            <ToolCallCard
                              key={tc.id}
                              call={tc}
                              onOpenMeeting={onOpenMeeting}
                            />
                          ))}
                          <CitedResponse
                            text={m.text || "​"}
                            sources={sources}
                            onOpenMeeting={onOpenMeeting}
                          />
                        </>
                      ) : (
                        m.text
                      )}
                    </MessageContent>
                  </Message>
                );
              })}
              {errorText && (
                <p className="text-xs text-red-500 dark:text-red-400">
                  {errorText}
                </p>
              )}
            </ConversationContent>
            <ConversationScrollButton />
          </Conversation>

          <div className="flex flex-col gap-1.5 border-t border-border p-2">
            {showContextChip && activeMeeting && (
              <div className="flex w-full min-w-0 items-center gap-1.5 px-1 text-xs">
                <FileText className="size-3.5 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate text-foreground">
                  {activeMeeting.title || "Untitled meeting"}
                </span>
                <button
                  type="button"
                  onClick={() => {
                    setDismissedContext(true);
                    textareaRef.current?.focus();
                  }}
                  aria-label="Remove meeting context"
                  className="-mr-0.5 shrink-0 rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
                >
                  <X className="size-3.5" />
                </button>
              </div>
            )}
            <PromptInput className="bg-background" onSubmit={handleSubmit}>
              <PromptInputBody>
                <PromptInputTextarea
                  ref={textareaRef}
                  placeholder={
                    canSubmit ? "Ask anything…" : "Download a model first…"
                  }
                  disabled={!canSubmit}
                />
              </PromptInputBody>
              <PromptInputFooter>
                <PromptInputTools>
                  <PromptInputButton
                    type="button"
                    variant={showContextChip ? "secondary" : "ghost"}
                    onClick={() => setDismissedContext((prev) => !prev)}
                    aria-pressed={showContextChip}
                    tooltip={
                      activeMeeting
                        ? showContextChip
                          ? "Remove meeting context"
                          : "Use current meeting as context"
                        : "No active meeting"
                    }
                  >
                    <Fullscreen className="size-4" />
                  </PromptInputButton>
                </PromptInputTools>
                <PromptInputSubmit
                  status={status}
                  onStop={handleStop}
                  disabled={!canSubmit}
                />
              </PromptInputFooter>
            </PromptInput>
          </div>
        </div>
      )}

      <Dialog
        open={confirmCloseOpen}
        onOpenChange={(next) => setConfirmCloseOpen(next ?? false)}
      >
        <DialogContent showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>Close chat?</DialogTitle>
            <DialogDescription>
              This conversation isn't saved. If you close the chat, the
              messages will be lost.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setConfirmCloseOpen(false)}
            >
              Keep chatting
            </Button>
            <Button
              type="button"
              variant="destructive"
              onClick={() => {
                setConfirmCloseOpen(false);
                setOpen(false);
              }}
            >
              Close chat
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

/**
 * Streamdown-based renderer for assistant prose that may contain `[N]` citation
 * markers. The rehype plugin rewrites those into `<cite data-citation="N">[N]</cite>`
 * elements, which we override to render a hoverable, clickable `CitationChip`.
 */
function CitedResponse({
  text,
  sources,
  onOpenMeeting,
}: {
  text: string;
  sources: SearchHit[];
  onOpenMeeting?: (meetingId: string) => void;
}) {
  return (
    <Streamdown
      className="size-full [&>*:first-child]:mt-0 [&>*:last-child]:mb-0"
      rehypePlugins={[rehypeCitations]}
      components={{
        cite: (props) => (
          <CitationChip
            data-citation={
              (props as { "data-citation"?: string })["data-citation"]
            }
            sources={sources}
            onOpenMeeting={onOpenMeeting}
          >
            {props.children}
          </CitationChip>
        ),
      }}
    >
      {text}
    </Streamdown>
  );
}

function ToolCallCard({
  call,
  onOpenMeeting,
}: {
  call: ToolCall;
  onOpenMeeting?: (meetingId: string) => void;
}) {
  const isSearch = call.name === "search_meetings";
  let query = "";
  if (call.args) {
    try {
      query = (JSON.parse(call.args)?.query as string | undefined) ?? "";
    } catch {
      query = "";
    }
  }

  return (
    <div className="mb-2 flex flex-col gap-1.5 rounded-md border border-border bg-muted/40 px-2.5 py-2 text-xs">
      <div className="flex items-center gap-1.5 text-muted-foreground">
        {call.status === "running" ? (
          <Loader2 className="size-3.5 shrink-0 animate-spin" />
        ) : (
          <Search className="size-3.5 shrink-0" />
        )}
        <span className="min-w-0 flex-1 truncate">
          {isSearch
            ? call.status === "running"
              ? query
                ? `Searching meetings: ${query}`
                : "Searching meetings…"
              : `Search results${query ? `: ${query}` : ""}`
            : call.name}
        </span>
        {call.status === "done" && call.hits && (
          <span className="shrink-0 tabular-nums">
            {call.hits.length} {call.hits.length === 1 ? "result" : "results"}
          </span>
        )}
      </div>
      {call.status === "done" && call.hits && call.hits.length > 0 && (
        <ul className="flex flex-col gap-1">
          {call.hits.map((hit, i) => (
            <li key={`${hit.meeting_id}-${hit.kind}-${i}`}>
              <button
                type="button"
                onClick={() => onOpenMeeting?.(hit.meeting_id)}
                disabled={!onOpenMeeting}
                className={cn(
                  "flex w-full flex-col gap-0.5 rounded border border-border/60 bg-background/60 px-2 py-1.5 text-left transition-colors",
                  onOpenMeeting && "hover:border-border hover:bg-background",
                  !onOpenMeeting && "cursor-default",
                )}
              >
                <div className="flex items-center gap-1.5">
                  <span className="min-w-0 flex-1 truncate font-medium text-foreground">
                    {hit.meeting_title || "Untitled meeting"}
                  </span>
                  <span
                    className={cn(
                      "shrink-0 rounded px-1 py-0.5 text-[10px] uppercase tracking-wide",
                      hit.kind === "summary"
                        ? "bg-blue-500/10 text-blue-600 dark:text-blue-400"
                        : "bg-muted text-muted-foreground",
                    )}
                  >
                    {hit.kind}
                  </span>
                </div>
                <div
                  className="line-clamp-2 text-muted-foreground [&_mark]:rounded [&_mark]:bg-yellow-200/60 [&_mark]:px-0.5 [&_mark]:text-foreground dark:[&_mark]:bg-yellow-300/30"
                  dangerouslySetInnerHTML={{ __html: hit.snippet }}
                />
              </button>
            </li>
          ))}
        </ul>
      )}
      {call.status === "done" && (!call.hits || call.hits.length === 0) && (
        <p className="text-muted-foreground italic">No matching meetings.</p>
      )}
    </div>
  );
}

export default Chatbot;
