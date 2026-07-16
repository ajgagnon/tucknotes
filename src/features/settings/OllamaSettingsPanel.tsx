import { useCallback, useEffect, useState } from "react";
import { RotateCw } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import {
  detectOllama,
  listOllamaModels,
  formatSize,
  type LlmEngineSettings,
  type OllamaModelInfo,
} from "@/features/models";

type ConnectionState =
  | { kind: "checking" }
  | { kind: "ok"; version: string | null }
  | { kind: "unreachable" };

/** Connection + model picker for the Ollama engine. Shown by
 *  `LlmEngineSection` when the provider is `ollama`. */
export function OllamaSettingsPanel({
  engine,
  save,
  disabled,
}: {
  engine: LlmEngineSettings;
  save: (next: LlmEngineSettings) => Promise<boolean>;
  disabled?: boolean;
}) {
  const [draftUrl, setDraftUrl] = useState(engine.ollama_base_url);
  const [connection, setConnection] = useState<ConnectionState>({
    kind: "checking",
  });
  const [models, setModels] = useState<OllamaModelInfo[] | null>(null);
  const [testing, setTesting] = useState(false);

  /** Probe `url` and, when reachable, refresh the installed-model list. */
  const refresh = useCallback(async (url: string): Promise<boolean> => {
    setConnection({ kind: "checking" });
    const status = await detectOllama(url);
    if (!status.reachable) {
      setConnection({ kind: "unreachable" });
      setModels(null);
      return false;
    }
    setConnection({ kind: "ok", version: status.version });
    try {
      setModels(await listOllamaModels(url));
    } catch {
      setModels([]);
    }
    return true;
  }, []);

  useEffect(() => {
    void refresh(engine.ollama_base_url);
  }, [refresh, engine.ollama_base_url]);

  const handleTest = async () => {
    setTesting(true);
    try {
      const trimmed = draftUrl.trim();
      const ok = await refresh(trimmed);
      if (ok && trimmed !== engine.ollama_base_url) {
        await save({ ...engine, ollama_base_url: trimmed });
      }
    } finally {
      setTesting(false);
    }
  };

  const selectedMissing =
    engine.ollama_model !== null &&
    models !== null &&
    !models.some((m) => m.name === engine.ollama_model);

  return (
    <div className="flex flex-col gap-4 rounded-lg border p-4">
      <div className="grid gap-2">
        <Label htmlFor="ollama-base-url">Server address</Label>
        <div className="flex gap-2">
          <Input
            id="ollama-base-url"
            value={draftUrl}
            onChange={(e) => setDraftUrl(e.target.value)}
            placeholder="http://localhost:11434"
            disabled={disabled}
            spellCheck={false}
          />
          <Button
            type="button"
            variant="outline"
            onClick={() => void handleTest()}
            disabled={disabled || testing || !draftUrl.trim()}
            className="shrink-0"
          >
            {testing && <Spinner className="size-3.5" />}
            Test
          </Button>
        </div>
        {connection.kind === "ok" && (
          <div>
            <Badge variant="outline">
              Connected{connection.version ? ` · Ollama ${connection.version}` : ""}
            </Badge>
          </div>
        )}
      </div>

      {connection.kind === "unreachable" && (
        <Alert variant="destructive">
          <AlertTitle>Can't reach Ollama</AlertTitle>
          <AlertDescription>
            Nothing answered at {engine.ollama_base_url}. Make sure Ollama is
            running, then test the connection again.
          </AlertDescription>
        </Alert>
      )}

      <div className="grid gap-2">
        <div className="flex items-center justify-between">
          <Label htmlFor="ollama-model-select">Model</Label>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-xs text-muted-foreground"
            onClick={() => void refresh(engine.ollama_base_url)}
            disabled={disabled || connection.kind === "checking"}
          >
            <RotateCw className="size-3" />
            Refresh
          </Button>
        </div>
        <Select
          value={engine.ollama_model ?? ""}
          onValueChange={(name) => void save({ ...engine, ollama_model: name })}
          disabled={disabled || connection.kind !== "ok" || !models?.length}
        >
          <SelectTrigger id="ollama-model-select" className="w-full">
            <SelectValue
              placeholder={
                connection.kind !== "ok"
                  ? "Connect to Ollama to choose a model"
                  : models?.length
                    ? "Choose a model"
                    : "No models installed"
              }
            />
          </SelectTrigger>
          <SelectContent>
            {/* Keep a vanished-but-selected model visible so the trigger
                doesn't render empty. */}
            {selectedMissing && engine.ollama_model && (
              <SelectItem value={engine.ollama_model}>
                {engine.ollama_model}
              </SelectItem>
            )}
            {models?.map((m) => (
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
        {selectedMissing && (
          <p className="text-xs text-destructive">
            {engine.ollama_model} is no longer installed in Ollama — pick
            another model.
          </p>
        )}
        {connection.kind === "ok" && models !== null && models.length === 0 && (
          <p className="text-xs text-muted-foreground">
            No models installed. Pull one in a terminal (for example{" "}
            <code className="font-mono">ollama pull qwen3:4b</code>), then
            refresh.
          </p>
        )}
        <p className="text-xs text-muted-foreground">
          Summaries and chat run on your Ollama server. Quality depends on the
          model you pick — instruction-tuned models of 4B parameters or more
          work best.
        </p>
      </div>
    </div>
  );
}
