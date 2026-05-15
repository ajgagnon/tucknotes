import { useCallback, useRef } from "react";

const CANNED_REPLIES = [
  "Sure — I can help with that. Here are a few ideas:\n\n- Capture the meeting with one click\n- Summarize the transcript into key points\n- Surface action items automatically\n\nLet me know which one to start with.",
  "Great question. A few things I'd suggest looking at:\n\n1. Recent meetings in the sidebar\n2. The action items panel\n3. Search (⌘K) to jump to a specific note\n\nWant me to walk through any of these?",
  "Happy to help! You can ask me about:\n\n- **Recording** — start, stop, or pause a meeting\n- **Search** — find meetings by title or content\n- **Settings** — adjust transcription and theme\n\nWhich would you like to explore?",
];

const CHUNK_SIZE = 3;
const CHUNK_DELAY_MS = 25;

export type StreamChunk = (chunk: string) => void;

export function useMockStream() {
  const replyIndex = useRef(0);
  const cancelRef = useRef<(() => void) | null>(null);

  const streamReply = useCallback(
    async (onChunk: StreamChunk): Promise<void> => {
      cancelRef.current?.();

      const text = CANNED_REPLIES[replyIndex.current % CANNED_REPLIES.length];
      replyIndex.current += 1;

      let cancelled = false;
      cancelRef.current = () => {
        cancelled = true;
      };

      for (let i = 0; i < text.length; i += CHUNK_SIZE) {
        if (cancelled) return;
        onChunk(text.slice(i, i + CHUNK_SIZE));
        await new Promise((resolve) => setTimeout(resolve, CHUNK_DELAY_MS));
      }
    },
    [],
  );

  const stop = useCallback(() => {
    cancelRef.current?.();
  }, []);

  return { streamReply, stop };
}
