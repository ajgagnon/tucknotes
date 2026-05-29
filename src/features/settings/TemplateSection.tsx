import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";
import type { TemplateInfo } from "@/features/meetings/types";

/// DOM id of this settings section, so other views (e.g. the summary template
/// dropdown's "Edit templates" link) can deep-link and scroll to it.
export const SETTINGS_SECTION_TEMPLATES = "settings-templates";

/// App-wide default summary template picker. The chosen template is applied to
/// meetings that don't yet have one of their own; per-meeting selection lives
/// in the summary footer.
export function TemplateSection({
  className,
  disabled,
}: {
  className?: string;
  disabled?: boolean;
}) {
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [selected, setSelected] = useState<string>("default");

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [list, current] = await Promise.all([
          invoke<TemplateInfo[]>("list_summary_templates"),
          invoke<string | null>("get_default_template"),
        ]);
        if (cancelled) return;
        setTemplates(list);
        setSelected(current ?? "default");
      } catch {
        // Leave defaults; the per-meeting picker still works.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  async function handleChange(value: string) {
    setSelected(value);
    try {
      await invoke("set_default_template", { template: value });
    } catch {
      // Non-fatal — the selection re-syncs from settings on next load.
    }
  }

  const description = templates.find((t) => t.id === selected)?.description;

  return (
    <section id={SETTINGS_SECTION_TEMPLATES} className={cn(className)}>
      <h2 className="text-sm font-medium text-muted-foreground mb-4">
        Default Summary Template
      </h2>
      <Select
        value={selected}
        onValueChange={(value) => void handleChange(value as string)}
        disabled={!!disabled || templates.length === 0}
      >
        <SelectTrigger aria-label="Default summary template" className="w-full">
          <SelectValue>
            {(value: string | null) =>
              templates.find((t) => t.id === value)?.name ?? "Default"
            }
          </SelectValue>
        </SelectTrigger>
        <SelectContent>
          {templates.map((t) => (
            <SelectItem key={t.id} value={t.id}>
              {t.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {description && (
        <p className="mt-2 text-xs text-muted-foreground">{description}</p>
      )}
    </section>
  );
}
