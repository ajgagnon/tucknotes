import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
  FieldTitle,
} from "@/components/ui/field";
import {
  LLM_MODEL_CONFIG,
  useLlmEngine,
  type LlmProvider,
} from "@/features/models";
import { ModelSection } from "./ModelSection";
import { OllamaSettingsPanel } from "./OllamaSettingsPanel";

function ProviderCard({
  value,
  title,
  description,
}: {
  value: LlmProvider;
  title: string;
  description: string;
}) {
  const id = `llm-provider-${value}`;
  return (
    <FieldLabel htmlFor={id} className="w-full">
      <Field orientation="horizontal">
        <RadioGroupItem value={value} id={id} className="mt-0.5" />
        <FieldContent>
          <FieldTitle>{title}</FieldTitle>
          <FieldDescription>{description}</FieldDescription>
        </FieldContent>
      </Field>
    </FieldLabel>
  );
}

/** Summarization engine choice: the built-in downloaded model (llama.cpp
 *  in-process) or a user-managed local Ollama server. */
export function LlmEngineSection({ disabled }: { disabled?: boolean }) {
  const { engine, save } = useLlmEngine();

  return (
    <section>
      <h2 className="text-sm font-medium text-muted-foreground mb-4">
        Summarization Model
      </h2>

      {engine === null ? (
        <div className="flex flex-col gap-3">
          <Skeleton className="h-16 rounded-lg" />
          <Skeleton className="h-16 rounded-lg" />
        </div>
      ) : (
        <div className="flex flex-col gap-4">
          <RadioGroup
            value={engine.provider}
            onValueChange={(provider) =>
              void save({ ...engine, provider: provider as LlmProvider })
            }
            disabled={disabled}
            className="flex flex-col gap-2 w-full"
          >
            <ProviderCard
              value="built_in"
              title="Built-in model"
              description="Download a model that runs privately inside TuckNotes."
            />
            <ProviderCard
              value="ollama"
              title="Ollama"
              description="Use a model from a local Ollama server you manage."
            />
          </RadioGroup>

          {engine.provider === "built_in" ? (
            <ModelSection
              config={LLM_MODEL_CONFIG}
              radioIdPrefix="llm-model"
              disabled={disabled}
            />
          ) : (
            <OllamaSettingsPanel
              engine={engine}
              save={save}
              disabled={disabled}
            />
          )}
        </div>
      )}
    </section>
  );
}
