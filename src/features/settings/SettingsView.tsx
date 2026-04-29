import { AppearanceSection } from "./AppearanceSection";
import { ModelSection } from "./ModelSection";
import { LLM_MODEL_CONFIG, WHISPER_MODEL_CONFIG } from "@/features/models";
import { useRecording } from "@/features/recording";
import { useSummarizationActive } from "./use-summarization-active";

function SettingsView() {
  const { recording, paused } = useRecording();
  const summarizing = useSummarizationActive();

  return (
    <div className="h-full overflow-auto">
      <div className="max-w-2xl mx-auto p-8">
        <AppearanceSection />
        <ModelSection
          title="Transcription Model"
          config={WHISPER_MODEL_CONFIG}
          radioIdPrefix="model"
          disabled={recording || paused}
        />
        <ModelSection
          title="Summarization Model"
          config={LLM_MODEL_CONFIG}
          radioIdPrefix="llm-model"
          className="mt-8"
          disabled={summarizing}
        />
      </div>
    </div>
  );
}

export default SettingsView;
