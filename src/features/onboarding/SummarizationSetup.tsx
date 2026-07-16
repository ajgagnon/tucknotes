import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DownloadCloud, Server, Sparkles } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type {
  LlmEngineSettings,
  ModelInfo,
  OllamaModelInfo,
} from "@/features/models";
import { detectOllama, formatSize, listOllamaModels } from "@/features/models";
import OnboardingShell from "./OnboardingShell";

interface SummarizationSetupProps {
  onComplete: () => void;
}

/** Sentinel selection id for the "use your Ollama" card; can't collide with
 *  built-in model ids (enum variant names). */
const OLLAMA_OPTION_ID = "__ollama__";

interface DetectedOllama {
  version: string | null;
  models: OllamaModelInfo[];
}

function cardClass(isSelected: boolean): string {
  return `rounded-xl p-5 px-6 text-left transition-all duration-200 cursor-pointer ${
    isSelected
      ? "border-2 border-primary bg-primary/5 dark:bg-primary/10"
      : "border border-border bg-muted/40 hover:border-foreground/20"
  }`;
}

function RadioDot({ isSelected }: { isSelected: boolean }) {
  return (
    <div
      className={`w-5 h-5 rounded-full border-2 flex items-center justify-center shrink-0 transition-colors ${
        isSelected ? "border-primary bg-primary" : "border-border"
      }`}
    >
      {isSelected && (
        <div className="w-2 h-2 rounded-full bg-primary-foreground" />
      )}
    </div>
  );
}

function SummarizationSetup({ onComplete }: SummarizationSetupProps) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [ollama, setOllama] = useState<DetectedOllama | null>(null);
  const [ollamaModel, setOllamaModel] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<ModelInfo[]>("list_available_llm_models").then((list) => {
      setModels(list);
      // Built-in stays the pre-selected, recommended path even when Ollama is
      // detected — prompts and tool-call parsing are tuned for these models.
      const initial = list.find((m) => m.recommended) ?? list[0];
      if (initial) setSelectedId((prev) => prev ?? initial.id);
    });

    // Probe for a local Ollama server (saved/default base URL). Only offer it
    // when it's reachable and has at least one model installed; otherwise the
    // full setup lives in Settings.
    detectOllama()
      .then(async (status) => {
        if (!status.reachable) return;
        const installed = await listOllamaModels().catch(
          () => [] as OllamaModelInfo[],
        );
        if (installed.length === 0) return;
        setOllama({ version: status.version, models: installed });
        setOllamaModel(installed[0].name);
      })
      .catch(() => {});
  }, []);

  async function handleContinue() {
    if (!selectedId) return;
    setSubmitting(true);
    setError(null);
    try {
      const engine = await invoke<LlmEngineSettings>("get_llm_engine_settings");
      if (selectedId === OLLAMA_OPTION_ID) {
        if (!ollamaModel) return;
        await invoke("set_llm_engine_settings", {
          engine: { ...engine, provider: "ollama", ollama_model: ollamaModel },
        });
        onComplete();
        return;
      }

      // Explicitly pin the provider: a half-configured Ollama choice from an
      // earlier run must not shadow the model download we're about to start.
      await invoke("set_llm_engine_settings", {
        engine: { ...engine, provider: "built_in" },
      });
      await invoke("set_selected_llm_model", { modelId: selectedId });
      // Fire-and-forget: the Tauri command keeps running on the tokio runtime
      // after we abandon the Promise, so the download persists into the app.
      void invoke("download_llm_model", { modelId: selectedId }).catch(
        () => {},
      );
      onComplete();
    } catch (err) {
      const e = err as { message?: string };
      setError(e.message ?? "Could not save your selection. Please try again.");
      setSubmitting(false);
    }
  }

  const ollamaSelected = selectedId === OLLAMA_OPTION_ID;

  return (
    <OnboardingShell
      icon={<Sparkles strokeWidth={1.75} />}
      title="Choose a summarization model"
      description="Used to summarize and title your meeting transcripts."
    >
      <div className="flex flex-col gap-3 w-full text-left">
        {models.map((model) => {
          const isSelected = selectedId === model.id;
          return (
            <button
              key={model.id}
              onClick={() => setSelectedId(model.id)}
              className={cardClass(isSelected)}
            >
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-2">
                  <h3 className="text-[0.95rem] font-semibold">{model.name}</h3>
                  {model.recommended && (
                    <Badge variant="outline">Recommended</Badge>
                  )}
                </div>
                <RadioDot isSelected={isSelected} />
              </div>
              <p className="text-sm text-muted-foreground leading-relaxed mb-2">
                {model.description}
              </p>
              <span className="text-xs font-medium text-muted-foreground">
                {formatSize(model.size_bytes)}
              </span>
            </button>
          );
        })}

        {ollama && (
          // A div (not <button>) so the nested model Select stays clickable.
          <div
            role="button"
            tabIndex={0}
            onClick={() => setSelectedId(OLLAMA_OPTION_ID)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                setSelectedId(OLLAMA_OPTION_ID);
              }
            }}
            className={cardClass(ollamaSelected)}
          >
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                <h3 className="text-[0.95rem] font-semibold">Use Ollama</h3>
                <Badge variant="outline">Detected</Badge>
              </div>
              <RadioDot isSelected={ollamaSelected} />
            </div>
            <p className="text-sm text-muted-foreground leading-relaxed mb-2">
              {`Ollama${ollama.version ? ` ${ollama.version}` : ""} is running on
              this Mac with ${ollama.models.length} model${
                ollama.models.length === 1 ? "" : "s"
              } installed — no download needed.`}
            </p>
            {ollamaSelected && (
              <Select
                value={ollamaModel ?? undefined}
                onValueChange={setOllamaModel}
              >
                <SelectTrigger
                  className="w-full mt-1"
                  onClick={(e) => e.stopPropagation()}
                >
                  <SelectValue placeholder="Choose a model" />
                </SelectTrigger>
                <SelectContent>
                  {ollama.models.map((m) => (
                    <SelectItem key={m.name} value={m.name}>
                      <span>{m.name}</span>
                      <span className="text-muted-foreground text-xs">
                        {[m.parameter_size, formatSize(m.size_bytes)]
                          .filter(Boolean)
                          .join(" · ")}
                      </span>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          </div>
        )}
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}

      <Button
        className="w-full h-11 rounded-xl text-[0.95rem] font-semibold"
        onClick={handleContinue}
        disabled={
          !selectedId || submitting || (ollamaSelected && !ollamaModel)
        }
      >
        Choose & Continue
      </Button>

      {ollamaSelected ? (
        <Alert className="text-left">
          <Server />
          <AlertTitle>Runs on your Ollama server</AlertTitle>
          <AlertDescription>
            Summaries use the Ollama model you picked. You can switch models or
            go back to the built-in one anytime in Settings.
          </AlertDescription>
        </Alert>
      ) : (
        <Alert className="text-left">
          <DownloadCloud />
          <AlertTitle>Downloads in the background</AlertTitle>
          <AlertDescription>
            You can start using the app right away — summarization will be
            available once the download finishes.
          </AlertDescription>
        </Alert>
      )}
    </OnboardingShell>
  );
}

export default SummarizationSetup;
