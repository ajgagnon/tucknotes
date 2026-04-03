import { AppearanceSection } from "@/components/settings/AppearanceSection";
import { ModelSection } from "@/components/settings/ModelSection";
import { LLM_MODEL_CONFIG, WHISPER_MODEL_CONFIG } from "@/hooks/useModelManager";

function SettingsView() {
  return (
    <div className="h-full overflow-auto">
      <div className="max-w-2xl mx-auto p-8">
        <AppearanceSection />
        <ModelSection
          title="Transcription Model"
          config={WHISPER_MODEL_CONFIG}
          radioIdPrefix="model"
        />
        <ModelSection
          title="Summarization Model"
          config={LLM_MODEL_CONFIG}
          radioIdPrefix="llm-model"
          className="mt-8"
        />
      </div>
    </div>
  );
}

export default SettingsView;
