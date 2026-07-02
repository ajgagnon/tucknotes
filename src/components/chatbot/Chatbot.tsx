import { useCallback, useEffect, useRef, useState } from "react";
import { FileText, Fullscreen, MessageCircle, Plus, X } from "lucide-react";

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
  Context,
  ContextContent,
  ContextContentBody,
  ContextContentHeader,
  ContextInputUsage,
  ContextOutputUsage,
  ContextTrigger,
} from "@/components/ai-elements/context";
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

import { useChatSession } from "./use-chat-session";
import { ChatMessages } from "./ChatMessages";
import { useAskTuckRequest } from "./ask-tuck-context";

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
  const [dismissedContext, setDismissedContext] = useState(false);
  const [confirmNewChatOpen, setConfirmNewChatOpen] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const { messages, status, usage, modelReady, submit, stop, reset } =
    useChatSession();
  const askTuckRequest = useAskTuckRequest();

  const requestClose = useCallback(() => setOpen(false), []);

  // External "Ask Tuck about this" requests (e.g. from an AI-summary block):
  // open the panel and pre-fill the textarea without sending. Keyed on `nonce`
  // so repeat requests re-trigger even when the panel is already open.
  useEffect(() => {
    if (!askTuckRequest) return;
    setOpen(true);
    // Double rAF: after setOpen(true), the panel (and textarea) must mount on
    // the next render before we can set its value.
    let raf2 = 0;
    const raf1 = requestAnimationFrame(() => {
      raf2 = requestAnimationFrame(() => {
        const ta = textareaRef.current;
        if (!ta) return;
        ta.value = askTuckRequest.text;
        // Notify any listeners and trigger `field-sizing-content` auto-resize.
        ta.dispatchEvent(new Event("input", { bubbles: true }));
        ta.focus();
        ta.setSelectionRange(ta.value.length, ta.value.length);
      });
    });
    return () => {
      cancelAnimationFrame(raf1);
      if (raf2) cancelAnimationFrame(raf2);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [askTuckRequest?.nonce]);

  const handleNewChat = useCallback(() => {
    if (messages.length === 0) return;
    setConfirmNewChatOpen(true);
  }, [messages.length]);

  const confirmNewChat = useCallback(() => {
    reset();
    setConfirmNewChatOpen(false);
    requestAnimationFrame(() => textareaRef.current?.focus());
  }, [reset]);

  useEffect(() => {
    if (!open) return;
    setDismissedContext(false);
    const raf = requestAnimationFrame(() => textareaRef.current?.focus());
    return () => cancelAnimationFrame(raf);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        // Escape dismisses the new-chat confirmation if it's up; otherwise closes the panel.
        if (confirmNewChatOpen) return;
        requestClose();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, confirmNewChatOpen, requestClose]);

  const showContextChip = open && !!activeMeeting && !dismissedContext;
  const canSubmit = modelReady === true;

  const handleSubmit = useCallback(
    async (message: PromptInputMessage) => {
      const text = message.text?.trim();
      if (!text) return;
      requestAnimationFrame(() => textareaRef.current?.focus());
      const meetingId =
        showContextChip && activeMeeting ? activeMeeting.id : null;
      await submit(text, meetingId);
    },
    [submit, showContextChip, activeMeeting],
  );

  return (
    <>
      {!open && (
        <Button
          type="button"
          onClick={() => setOpen(true)}
          aria-label="Ask Tuck"
          className="fixed bottom-4 right-4 z-50 h-10 gap-2 rounded-full px-4 shadow-lg"
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
            <div className="flex items-center gap-1">
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                onClick={handleNewChat}
                disabled={messages.length === 0}
                aria-label="Start new chat"
              >
                <Plus className="size-4" />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                onClick={requestClose}
                aria-label="Close chat"
              >
                <X className="size-4" />
              </Button>
            </div>
          </header>

          <ChatMessages
            messages={messages}
            modelReady={modelReady}
            onOpenSettings={onOpenSettings}
            onOpenMeeting={onOpenMeeting}
          />

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
                  {usage &&
                    usage.max_tokens > 0 &&
                    (usage.prompt_tokens + usage.completion_tokens) /
                      usage.max_tokens >=
                      0.8 && (
                      <Context
                        usedTokens={
                          usage.prompt_tokens + usage.completion_tokens
                        }
                        maxTokens={usage.max_tokens}
                        usage={{
                          inputTokens: usage.prompt_tokens,
                          outputTokens: usage.completion_tokens,
                        }}
                      >
                        <ContextTrigger />
                        <ContextContent>
                          <ContextContentHeader />
                          <ContextContentBody>
                            <ContextInputUsage />
                            <ContextOutputUsage />
                          </ContextContentBody>
                        </ContextContent>
                      </Context>
                    )}
                </PromptInputTools>
                <PromptInputSubmit
                  status={status}
                  onStop={stop}
                  disabled={!canSubmit}
                />
              </PromptInputFooter>
            </PromptInput>
          </div>
        </div>
      )}

      <Dialog
        open={confirmNewChatOpen}
        onOpenChange={(next) => setConfirmNewChatOpen(next ?? false)}
      >
        <DialogContent showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>Start a new chat?</DialogTitle>
            <DialogDescription>
              Your current conversation will be cleared and can't be recovered.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setConfirmNewChatOpen(false)}
            >
              Keep chatting
            </Button>
            <Button
              type="button"
              variant="destructive"
              onClick={confirmNewChat}
            >
              Start new chat
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

export default Chatbot;
