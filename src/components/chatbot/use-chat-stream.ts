import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DownloadProgress } from "@/features/models";

export type ChatRole = "user" | "assistant";

type ChatTokenPayload = { chat_id: string; token: string };
type ChatCompletePayload = { chat_id: string };
type ChatErrorPayload = { chat_id: string; error: string };

type SendOpts = {
  meetingId: string | null;
  history: { role: ChatRole; text: string }[];
  onToken: (chunk: string) => void;
  onComplete: () => void;
  onError: (error: string) => void;
};

const newId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : Math.random().toString(36).slice(2);

export function useChatStream() {
  const [modelReady, setModelReady] = useState<boolean | null>(null);
  const inflightChatId = useRef<string | null>(null);
  const inflightCleanup = useRef<(() => void) | null>(null);

  const checkLlmModel = useCallback(async () => {
    try {
      const selected = await invoke<string | null>("get_selected_llm_model");
      if (!selected) {
        setModelReady(false);
        return;
      }
      const ready = await invoke<boolean>("get_llm_model_status", {
        modelId: selected,
      });
      setModelReady(ready);
    } catch {
      setModelReady(false);
    }
  }, []);

  useEffect(() => {
    void checkLlmModel();
  }, [checkLlmModel]);

  // Re-check when a model finishes downloading (mirrors useMeetingSummarization).
  useEffect(() => {
    const unlisten = listen<DownloadProgress>(
      "llm-model:download-progress",
      (event) => {
        const { downloaded_bytes, total_bytes } = event.payload;
        if (total_bytes <= 0 || downloaded_bytes < total_bytes) return;
        void checkLlmModel();
        setTimeout(() => void checkLlmModel(), 250);
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [checkLlmModel]);

  const send = useCallback(async (opts: SendOpts) => {
    const chatId = newId();
    inflightChatId.current = chatId;

    const unlistenToken = await listen<ChatTokenPayload>(
      "chat:token",
      (event) => {
        if (event.payload.chat_id !== chatId) return;
        opts.onToken(event.payload.token);
      },
    );
    const unlistenComplete = await listen<ChatCompletePayload>(
      "chat:complete",
      (event) => {
        if (event.payload.chat_id !== chatId) return;
        cleanup();
        opts.onComplete();
      },
    );
    const unlistenError = await listen<ChatErrorPayload>(
      "chat:error",
      (event) => {
        if (event.payload.chat_id !== chatId) return;
        cleanup();
        opts.onError(event.payload.error);
      },
    );

    const cleanup = () => {
      if (inflightChatId.current === chatId) {
        inflightChatId.current = null;
        inflightCleanup.current = null;
      }
      unlistenToken();
      unlistenComplete();
      unlistenError();
    };
    inflightCleanup.current = cleanup;

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

  return { send, stop, modelReady, recheckModel: checkLlmModel };
}
