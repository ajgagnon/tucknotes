import { AppearanceSection } from "./AppearanceSection";
import { ModelSection } from "./ModelSection";
import { UpdateSection } from "./UpdateSection";
import { LLM_MODEL_CONFIG, WHISPER_MODEL_CONFIG } from "@/features/models";
import { useRecording } from "@/features/recording";
import { useSummarizationActive } from "./use-summarization-active";
import { LicenseSection } from "@/features/licensing";

function SettingsView() {
  const { recording, paused } = useRecording();
  const summarizing = useSummarizationActive();

  return (
    <div className="h-full overflow-auto">
      <div className="max-w-2xl mx-auto p-8 grid gap-8">
        <AppearanceSection />
        <LicenseSection />
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
          disabled={summarizing}
        />
        <UpdateSection />
      </div>
    </div>
  );
}

export default SettingsView;
