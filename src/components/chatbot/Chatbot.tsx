import { useCallback, useEffect, useRef, useState } from "react";
import { FileText, Fullscreen, MessageCircle, X } from "lucide-react";
import type { ChatStatus } from "ai";

import { Button } from "@/components/ui/button";
import {
  Conversation,
  ConversationContent,
  ConversationEmptyState,
  ConversationScrollButton,
} from "@/components/ai-elements/conversation";
import {
  Message,
  MessageContent,
  MessageResponse,
} from "@/components/ai-elements/message";
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

import { useChatStream } from "./use-chat-stream";

type ChatMessage = {
  id: string;
  role: "user" | "assistant";
  text: string;
};

const HISTORY_CAP = 12;

const newId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : Math.random().toString(36).slice(2);

type ChatbotProps = {
  activeMeeting?: { id: string; title: string | null } | null;
  onOpenSettings?: () => void;
};

export function Chatbot({ activeMeeting, onOpenSettings }: ChatbotProps = {}) {
  const [open, setOpen] = useState(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [status, setStatus] = useState<ChatStatus | undefined>(undefined);
  const [errorText, setErrorText] = useState<string | null>(null);
  const [dismissedContext, setDismissedContext] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const { send, stop, modelReady } = useChatStream();

  useEffect(() => {
    if (!open) {
      // Per-panel-open session: clear history + status on close.
      setMessages([]);
      setStatus(undefined);
      setErrorText(null);
      return;
    }
    setDismissedContext(false);
    const raf = requestAnimationFrame(() => textareaRef.current?.focus());
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", onKey);
    return () => {
      cancelAnimationFrame(raf);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const showContextChip = open && !!activeMeeting && !dismissedContext;
  const canSubmit = modelReady === true;

  const appendChunk = useCallback((id: string, chunk: string) => {
    setMessages((prev) =>
      prev.map((m) => (m.id === id ? { ...m, text: m.text + chunk } : m)),
    );
  }, []);

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
      });
    },
    [status, send, appendChunk, showContextChip, activeMeeting, messages, canSubmit],
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
            "fixed bottom-4 right-4 z-50 flex h-[560px] w-[380px] flex-col overflow-hidden rounded-xl border border-border bg-popover text-popover-foreground shadow-xl",
            "animate-in fade-in zoom-in-95 slide-in-from-bottom-2 duration-200",
          )}
        >
          <header className="flex h-11 shrink-0 items-center justify-between border-b border-border px-3">
            <span className="text-sm font-medium">Chat</span>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              onClick={() => setOpen(false)}
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
              {messages.map((m) => (
                <Message from={m.role} key={m.id}>
                  <MessageContent>
                    {m.role === "assistant" ? (
                      <MessageResponse>{m.text || "​"}</MessageResponse>
                    ) : (
                      m.text
                    )}
                  </MessageContent>
                </Message>
              ))}
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
            <PromptInput onSubmit={handleSubmit}>
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
                    disabled={!activeMeeting}
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
    </>
  );
}

export default Chatbot;
