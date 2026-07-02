import { useCallback, useState } from "react";
import type { ChatStatus } from "ai";
import { toastError } from "@/lib/toast";
import {
  useChatStream,
  type ChatUsagePayload,
  type SearchHit,
} from "./use-chat-stream";

export type ToolCall = {
  id: string;
  name: string;
  args: string;
  status: "running" | "done";
  hits?: SearchHit[];
};

export type ChatMessage = {
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

/**
 * Chat conversation state: the message list, streaming status, token usage,
 * and the send/stop/reset operations wired to the backend chat stream.
 */
export function useChatSession() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [status, setStatus] = useState<ChatStatus | undefined>(undefined);
  const [usage, setUsage] = useState<ChatUsagePayload | null>(null);
  const { send, stop: stopStream, modelReady } = useChatStream();

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

  const submit = useCallback(
    async (text: string, meetingId: string | null) => {
      if (!text || status === "submitted" || status === "streaming") return;
      if (modelReady !== true) return;

      const userMsg: ChatMessage = { id: newId(), role: "user", text };
      const assistantMsg: ChatMessage = {
        id: newId(),
        role: "assistant",
        text: "",
      };
      setMessages((prev) => [...prev, userMsg, assistantMsg]);

      setStatus("submitted");
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
          toastError(msg);
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
        onUsage: (next) => {
          setUsage(next);
        },
      });
    },
    [status, send, appendChunk, upsertToolCall, messages, modelReady],
  );

  const stop = useCallback(() => {
    stopStream();
    setStatus(undefined);
  }, [stopStream]);

  const reset = useCallback(() => {
    stopStream();
    setMessages([]);
    setStatus(undefined);
    setUsage(null);
  }, [stopStream]);

  return { messages, status, usage, modelReady, submit, stop, reset };
}
