import { useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { listenBatch } from "@/lib/tauri-events";
import { useLlmModelReady } from "@/features/models";

export type ChatRole = "user" | "assistant";

export type SearchHit = {
  meeting_id: string;
  meeting_title: string | null;
  meeting_created_at: number;
  kind: string;
  snippet: string;
  rank: number;
};

type ChatTokenPayload = { chat_id: string; token: string };
type ChatCompletePayload = { chat_id: string };
type ChatErrorPayload = { chat_id: string; error: string };
export type ChatUsagePayload = {
  chat_id: string;
  prompt_tokens: number;
  completion_tokens: number;
  max_tokens: number;
};
type ToolCallStartPayload = { chat_id: string; call_id: string; name: string };
type ToolCallArgsDeltaPayload = {
  chat_id: string;
  call_id: string;
  delta: string;
};
type ToolCallEndPayload = { chat_id: string; call_id: string };
type ToolResultPayload = {
  chat_id: string;
  call_id: string;
  name: string;
  hits: SearchHit[];
};

type SendOpts = {
  meetingId: string | null;
  history: { role: ChatRole; text: string }[];
  onToken: (chunk: string) => void;
  onComplete: () => void;
  onError: (error: string) => void;
  onToolCallStart?: (callId: string, name: string) => void;
  onToolCallArgsDelta?: (callId: string, delta: string) => void;
  onToolCallEnd?: (callId: string) => void;
  onToolResult?: (callId: string, name: string, hits: SearchHit[]) => void;
  onUsage?: (usage: ChatUsagePayload) => void;
};

const newId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : Math.random().toString(36).slice(2);

export function useChatStream() {
  const { ready: modelReady } = useLlmModelReady();
  const inflightChatId = useRef<string | null>(null);

  const send = useCallback(async (opts: SendOpts) => {
    const chatId = newId();
    inflightChatId.current = chatId;

    let unlisten: UnlistenFn = () => {};
    const cleanup = () => {
      if (inflightChatId.current === chatId) {
        inflightChatId.current = null;
      }
      unlisten();
    };

    unlisten = await listenBatch([
      listen<ChatTokenPayload>("chat:token", (event) => {
        if (event.payload.chat_id !== chatId) return;
        opts.onToken(event.payload.token);
      }),
      listen<ChatCompletePayload>("chat:complete", (event) => {
        if (event.payload.chat_id !== chatId) return;
        cleanup();
        opts.onComplete();
      }),
      listen<ChatErrorPayload>("chat:error", (event) => {
        if (event.payload.chat_id !== chatId) return;
        cleanup();
        opts.onError(event.payload.error);
      }),
      listen<ToolCallStartPayload>("chat:tool_call_start", (event) => {
        if (event.payload.chat_id !== chatId) return;
        opts.onToolCallStart?.(event.payload.call_id, event.payload.name);
      }),
      listen<ToolCallArgsDeltaPayload>("chat:tool_call_args_delta", (event) => {
        if (event.payload.chat_id !== chatId) return;
        opts.onToolCallArgsDelta?.(event.payload.call_id, event.payload.delta);
      }),
      listen<ToolCallEndPayload>("chat:tool_call_end", (event) => {
        if (event.payload.chat_id !== chatId) return;
        opts.onToolCallEnd?.(event.payload.call_id);
      }),
      listen<ToolResultPayload>("chat:tool_result", (event) => {
        if (event.payload.chat_id !== chatId) return;
        opts.onToolResult?.(
          event.payload.call_id,
          event.payload.name,
          event.payload.hits,
        );
      }),
      listen<ChatUsagePayload>("chat:usage", (event) => {
        if (event.payload.chat_id !== chatId) return;
        opts.onUsage?.(event.payload);
      }),
    ]);

    try {
      await invoke("chat_send_message", {
        chatId,
        meetingId: opts.meetingId,
        history: opts.history,
      });
      // The Rust command also emits chat:complete / chat:error before
      // returning; the listener handlers above already invoked cleanup.
    } catch (err) {
      // Network/IPC failure or unhandled error from Rust. The error event
      // may also have fired — defend against double-callback by checking
      // cleanup was already called.
      if (inflightChatId.current === chatId) {
        cleanup();
        opts.onError(err instanceof Error ? err.message : String(err));
      }
    }
  }, []);

  const stop = useCallback(() => {
    if (!inflightChatId.current) return;
    void invoke("chat_stop").catch(() => {
      /* swallow — UI will reset via complete/error listeners */
    });
  }, []);

  return { send, stop, modelReady };
}
