import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type PropsWithChildren,
} from "react";

/**
 * A request to open the Ask Tuck chat with pre-filled input. The `nonce`
 * increments on every call so consumers can re-trigger their prefill/focus
 * effect even when the same text is requested again (or the panel is already
 * open).
 */
export type AskTuckRequest = { text: string; nonce: number };

type AskTuckContextValue = {
  /** Open the Ask Tuck chat with `text` pre-filled (not auto-sent). */
  openAskTuck: (text: string) => void;
  /** Latest open request, consumed by the Chatbot. */
  request: AskTuckRequest | null;
};

const AskTuckContext = createContext<AskTuckContextValue | null>(null);

export function AskTuckProvider({ children }: PropsWithChildren) {
  const [request, setRequest] = useState<AskTuckRequest | null>(null);

  const openAskTuck = useCallback((text: string) => {
    setRequest((prev) => ({ text, nonce: (prev?.nonce ?? 0) + 1 }));
  }, []);

  const value = useMemo(() => ({ openAskTuck, request }), [openAskTuck, request]);

  return (
    <AskTuckContext.Provider value={value}>{children}</AskTuckContext.Provider>
  );
}

function useAskTuckContext(): AskTuckContextValue {
  const ctx = useContext(AskTuckContext);
  if (!ctx) {
    throw new Error("useAskTuck must be used within an <AskTuckProvider>");
  }
  return ctx;
}

/** For producers (e.g. summary block actions) that open the chat. */
export function useAskTuck() {
  return { openAskTuck: useAskTuckContext().openAskTuck };
}

/** For the Chatbot: the latest open request (or null). */
export function useAskTuckRequest() {
  return useAskTuckContext().request;
}
