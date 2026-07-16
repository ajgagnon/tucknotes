import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toastError } from "@/lib/toast";

export type LlmProvider = "built_in" | "ollama";

/** Mirrors `LlmEngineSettings` in src-tauri/src/commands/ollama.rs. */
export interface LlmEngineSettings {
  provider: LlmProvider;
  ollama_base_url: string;
  ollama_model: string | null;
}

export interface OllamaStatus {
  reachable: boolean;
  version: string | null;
}

export interface OllamaModelInfo {
  name: string;
  size_bytes: number;
  parameter_size: string | null;
  quantization: string | null;
  family: string | null;
}

/** Probe an Ollama server (saved base URL when omitted). Never throws. */
export function detectOllama(baseUrl?: string): Promise<OllamaStatus> {
  return invoke<OllamaStatus>("detect_ollama", { baseUrl: baseUrl ?? null });
}

/** List models installed on an Ollama server (saved base URL when omitted). */
export function listOllamaModels(baseUrl?: string): Promise<OllamaModelInfo[]> {
  return invoke<OllamaModelInfo[]>("list_ollama_models", {
    baseUrl: baseUrl ?? null,
  });
}

function errorMessage(err: unknown, fallback: string): string {
  const e = err as { message?: string };
  return e.message ?? fallback;
}

/**
 * The persisted LLM engine choice (built-in model vs. Ollama) and a `save`
 * that writes the whole settings object back. `engine` is `null` until the
 * first load completes.
 */
export function useLlmEngine() {
  const [engine, setEngine] = useState<LlmEngineSettings | null>(null);

  useEffect(() => {
    invoke<LlmEngineSettings>("get_llm_engine_settings")
      .then(setEngine)
      .catch((e) => console.error("get_llm_engine_settings:", e));
  }, []);

  const save = useCallback(
    async (next: LlmEngineSettings): Promise<boolean> => {
      const prev = engine;
      setEngine(next);
      try {
        await invoke("set_llm_engine_settings", { engine: next });
        return true;
      } catch (err) {
        setEngine(prev);
        toastError(errorMessage(err, "Failed to save engine settings."));
        return false;
      }
    },
    [engine],
  );

  return { engine, save };
}
