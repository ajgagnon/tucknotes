import { useCallback, useEffect, useRef, useState } from "react";
import { MessageCircle, X } from "lucide-react";
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
  PromptInputFooter,
  PromptInputSubmit,
  PromptInputTextarea,
  PromptInputTools,
  type PromptInputMessage,
} from "@/components/ai-elements/prompt-input";
import { cn } from "@/lib/utils";

import { useMockStream } from "./use-mock-stream";

type ChatMessage = {
  id: string;
  role: "user" | "assistant";
  text: string;
};

const newId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : Math.random().toString(36).slice(2);

export function Chatbot() {
  const [open, setOpen] = useState(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [status, setStatus] = useState<ChatStatus | undefined>(undefined);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const { streamReply, stop } = useMockStream();

  useEffect(() => {
    if (!open) return;
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

  const appendChunk = useCallback((id: string, chunk: string) => {
    setMessages((prev) =>
      prev.map((m) => (m.id === id ? { ...m, text: m.text + chunk } : m)),
    );
  }, []);

  const handleSubmit = useCallback(
    async (message: PromptInputMessage) => {
      const text = message.text?.trim();
      if (!text || status === "submitted" || status === "streaming") return;

      const userMsg: ChatMessage = { id: newId(), role: "user", text };
      const assistantMsg: ChatMessage = {
        id: newId(),
        role: "assistant",
        text: "",
      };
      setMessages((prev) => [...prev, userMsg, assistantMsg]);
      requestAnimationFrame(() => textareaRef.current?.focus());

      setStatus("submitted");
      await new Promise((r) => setTimeout(r, 200));
      setStatus("streaming");
      await streamReply((chunk) => appendChunk(assistantMsg.id, chunk));
      setStatus(undefined);
    },
    [status, streamReply, appendChunk],
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
            "fixed bottom-4 right-4 z-50 flex h-[560px] w-[380px] flex-col overflow-hidden rounded-xl border border-border bg-background shadow-xl",
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
              {messages.length === 0 ? (
                <ConversationEmptyState
                  icon={<MessageCircle className="size-8" />}
                  title="Ask me anything"
                  description="I'm here to help you get more out of your meetings."
                />
              ) : (
                messages.map((m) => (
                  <Message from={m.role} key={m.id}>
                    <MessageContent>
                      {m.role === "assistant" ? (
                        <MessageResponse>
                          {m.text || "​"}
                        </MessageResponse>
                      ) : (
                        m.text
                      )}
                    </MessageContent>
                  </Message>
                ))
              )}
            </ConversationContent>
            <ConversationScrollButton />
          </Conversation>

          <div className="border-t border-border p-2">
            <PromptInput onSubmit={handleSubmit}>
              <PromptInputBody>
                <PromptInputTextarea
                  ref={textareaRef}
                  placeholder="Ask anything…"
                />
              </PromptInputBody>
              <PromptInputFooter>
                <PromptInputTools />
                <PromptInputSubmit status={status} onStop={handleStop} />
              </PromptInputFooter>
            </PromptInput>
          </div>
        </div>
      )}
    </>
  );
}

export default Chatbot;
