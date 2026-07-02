import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { MeetingRow, TemplateInfo } from "./types";

/**
 * The built-in template list plus the picker selection for a meeting: the
 * meeting's stored template wins, then the app-wide default, then "default".
 * Switching the picker afterwards only mutates local state (applied on
 * Summarize).
 */
export function useSummaryTemplates(
  meeting: Pick<MeetingRow, "id" | "template">,
): {
  templates: TemplateInfo[];
  selectedTemplate: string;
  setSelectedTemplate: (templateId: string) => void;
} {
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [selectedTemplate, setSelectedTemplate] = useState<string>(
    meeting.template ?? "default",
  );

  // Load the (static) list of built-in templates once.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const list = await invoke<TemplateInfo[]>("list_summary_templates");
        if (!cancelled) setTemplates(list);
      } catch {
        // Picker just renders no options; Summarize still works (→ Default).
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Resolve the picker selection whenever the meeting changes: the meeting's
  // stored template wins, then the app-wide default, then "default".
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      if (meeting.template) {
        setSelectedTemplate(meeting.template);
        return;
      }
      try {
        const appDefault = await invoke<string | null>("get_default_template");
        if (!cancelled) setSelectedTemplate(appDefault ?? "default");
      } catch {
        if (!cancelled) setSelectedTemplate("default");
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meeting.id, meeting.template]);

  return { templates, selectedTemplate, setSelectedTemplate };
}
