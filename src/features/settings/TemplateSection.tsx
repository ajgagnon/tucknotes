import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { Pencil, RotateCcw, Trash2, Plus } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import type { TemplateInfo } from "@/features/meetings/types";

/// DOM id of this settings section, so other views (e.g. the summary template
/// dropdown's "Edit templates" link) can deep-link and scroll to it.
export const SETTINGS_SECTION_TEMPLATES = "settings-templates";

/// Summary template management: pick the app-wide default, and create / edit /
/// reset / delete templates. `onEditTemplate(id?)` opens the full-screen
/// editor (no id = create a new template).
export function TemplateSection({
  className,
  disabled,
  onEditTemplate,
}: {
  className?: string;
  disabled?: boolean;
  onEditTemplate: (id?: string) => void;
}) {
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [selected, setSelected] = useState<string>("default");

  const load = useCallback(async () => {
    try {
      const [list, current] = await Promise.all([
        invoke<TemplateInfo[]>("list_summary_templates"),
        invoke<string | null>("get_default_template"),
      ]);
      setTemplates(list);
      // If the stored default points at a now-deleted template, fall back.
      setSelected(
        current && list.some((t) => t.id === current) ? current : "default",
      );
    } catch {
      // Leave defaults; the per-meeting picker still works.
    }
  }, []);

  // Reloads on mount, including when the user returns from the editor view
  // (Settings unmounts while editing, so this re-runs and picks up edits).
  useEffect(() => {
    void load();
  }, [load]);

  async function handleChange(value: string) {
    setSelected(value);
    try {
      await invoke("set_default_template", { template: value });
    } catch {
      // Non-fatal — the selection re-syncs from settings on next load.
    }
  }

  async function handleReset(t: TemplateInfo) {
    const confirmed = await ask(
      `Reset "${t.name}" to its original built-in version? Your edits to it will be lost.`,
      { title: "Reset template?", kind: "warning" },
    );
    if (!confirmed) return;
    try {
      await invoke("reset_summary_template", { id: t.id });
      await load();
    } catch (e) {
      await message(typeof e === "string" ? e : "Failed to reset template.", {
        kind: "error",
      });
    }
  }

  async function handleDelete(t: TemplateInfo) {
    const confirmed = await ask(
      `Delete "${t.name}"? Meetings that used it will fall back to the Default template.`,
      { title: "Delete template?", kind: "warning" },
    );
    if (!confirmed) return;
    try {
      await invoke("delete_summary_template", { id: t.id });
      await load();
    } catch (e) {
      await message(typeof e === "string" ? e : "Failed to delete template.", {
        kind: "error",
      });
    }
  }

  const description = templates.find((t) => t.id === selected)?.description;

  return (
    <section id={SETTINGS_SECTION_TEMPLATES} className={cn(className)}>
      <h2 className="text-sm font-medium text-muted-foreground mb-4">
        Summary Templates
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
      <p className="mt-2 text-xs text-muted-foreground">
        {description
          ? `Default for new meetings — ${description}`
          : "The template applied to new meetings by default."}
      </p>

      <div className="mt-4 grid gap-2">
        {templates.map((t) => (
          <Card
            key={t.id}
            className="flex flex-row items-center justify-between gap-3 p-3"
          >
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <span className="truncate text-sm font-medium">{t.name}</span>
                {t.builtin && (
                  <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground">
                    Built-in
                  </span>
                )}
              </div>
              {t.description && (
                <p className="truncate text-xs text-muted-foreground">
                  {t.description}
                </p>
              )}
            </div>
            <div className="flex shrink-0 items-center gap-1">
              <Button
                variant="ghost"
                size="icon-sm"
                disabled={disabled}
                onClick={() => onEditTemplate(t.id)}
                aria-label={`Edit ${t.name}`}
              >
                <Pencil className="size-4" />
              </Button>
              {t.builtin ? (
                <Button
                  variant="ghost"
                  size="icon-sm"
                  disabled={disabled}
                  onClick={() => void handleReset(t)}
                  aria-label={`Reset ${t.name}`}
                >
                  <RotateCcw className="size-4" />
                </Button>
              ) : (
                <Button
                  variant="ghost"
                  size="icon-sm"
                  disabled={disabled}
                  onClick={() => void handleDelete(t)}
                  aria-label={`Delete ${t.name}`}
                >
                  <Trash2 className="size-4" />
                </Button>
              )}
            </div>
          </Card>
        ))}
      </div>

      <Button
        variant="outline"
        size="sm"
        className="mt-3"
        disabled={disabled}
        onClick={() => onEditTemplate(undefined)}
      >
        <Plus className="size-4" />
        New template
      </Button>
    </section>
  );
}
