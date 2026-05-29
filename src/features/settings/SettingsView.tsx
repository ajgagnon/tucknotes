import { useEffect } from "react";
import { AppearanceSection } from "./AppearanceSection";
import { ModelSection } from "./ModelSection";
import { TemplateSection } from "./TemplateSection";
import { UpdateSection } from "./UpdateSection";
import { LLM_MODEL_CONFIG, WHISPER_MODEL_CONFIG } from "@/features/models";
import { useRecording } from "@/features/recording";
import { useSummarizationActive } from "./use-summarization-active";
import { LicenseSection } from "@/features/licensing";

/// `section` (optional) is the DOM id of a section to scroll to on open, used
/// for deep-links like the summary template dropdown's "Edit templates" link.
function SettingsView({ section }: { section?: string }) {
  const { recording, paused } = useRecording();
  const summarizing = useSummarizationActive();

  useEffect(() => {
    if (!section) return;
    const scrollToSection = () =>
      document.getElementById(section)?.scrollIntoView({ block: "start" });
    // Scroll once after first paint, then again after async sections (model
    // lists) have loaded and settled their height.
    const raf = requestAnimationFrame(scrollToSection);
    const timer = window.setTimeout(scrollToSection, 250);
    return () => {
      cancelAnimationFrame(raf);
      clearTimeout(timer);
    };
  }, [section]);

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
        <TemplateSection disabled={summarizing} />
        <UpdateSection />
      </div>
    </div>
  );
}

export default SettingsView;
