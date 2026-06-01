import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { message } from "@tauri-apps/plugin-dialog";
import { ArrowLeft, ArrowUp, ArrowDown, Trash2, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Card } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import type { OwnedSection, OwnedTemplate } from "./template-types";

/// Monotonic counter for client-side section keys on freshly added sections.
let sectionSeq = 0;
function newSection(): OwnedSection {
  sectionSeq += 1;
  return { id: `new-${sectionSeq}`, heading: "", description: "", example: "" };
}

function blankTemplate(): OwnedTemplate {
  return {
    id: "",
    name: "",
    description: "",
    sections: [newSection()],
    builtin: false,
    template_example: null,
  };
}

/// Full-screen editor for creating or editing a summary template. `templateId`
/// undefined means "create new". On save/cancel it calls `onDone` to return to
/// the Templates settings section.
export function TemplateEditorView({
  templateId,
  onDone,
  onDirtyChange,
}: {
  templateId?: string;
  onDone: () => void;
  onDirtyChange: (dirty: boolean) => void;
}) {
  // Snapshot of the saved/initial template; used to detect unsaved edits.
  const initialRef = useRef<string | null>(
    templateId ? null : JSON.stringify(blankTemplate()),
  );
  const [template, setTemplate] = useState<OwnedTemplate | null>(
    templateId ? null : blankTemplate(),
  );
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!templateId) return;
    let cancelled = false;
    void (async () => {
      try {
        const t = await invoke<OwnedTemplate>("get_summary_template", {
          id: templateId,
        });
        if (!cancelled) {
          initialRef.current = JSON.stringify(t);
          setTemplate(t);
        }
      } catch {
        if (!cancelled) onDone();
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [templateId, onDone]);

  const isDirty =
    template != null &&
    initialRef.current != null &&
    JSON.stringify(template) !== initialRef.current;

  // Report dirtiness up so navigation can warn before discarding edits, and
  // clear the flag when the editor unmounts.
  useEffect(() => {
    onDirtyChange(isDirty);
    return () => onDirtyChange(false);
  }, [isDirty, onDirtyChange]);

  if (!template) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spinner />
      </div>
    );
  }

  const isNew = !templateId;

  function patch(changes: Partial<OwnedTemplate>) {
    setTemplate((t) => (t ? { ...t, ...changes } : t));
  }

  function patchSection(index: number, changes: Partial<OwnedSection>) {
    setTemplate((t) =>
      t
        ? {
            ...t,
            sections: t.sections.map((s, i) =>
              i === index ? { ...s, ...changes } : s,
            ),
          }
        : t,
    );
  }

  function moveSection(index: number, dir: -1 | 1) {
    setTemplate((t) => {
      if (!t) return t;
      const target = index + dir;
      if (target < 0 || target >= t.sections.length) return t;
      const sections = [...t.sections];
      [sections[index], sections[target]] = [sections[target], sections[index]];
      return { ...t, sections };
    });
  }

  function removeSection(index: number) {
    setTemplate((t) =>
      t ? { ...t, sections: t.sections.filter((_, i) => i !== index) } : t,
    );
  }

  function addSection() {
    setTemplate((t) =>
      t ? { ...t, sections: [...t.sections, newSection()] } : t,
    );
  }

  async function handleSave() {
    if (!template) return;
    // Mirror the backend validation so the user gets fast, inline feedback.
    if (!template.name.trim()) {
      await message("Please give the template a name.", { kind: "warning" });
      return;
    }
    if (template.sections.length === 0) {
      await message("Add at least one section.", { kind: "warning" });
      return;
    }
    const badSection = template.sections.find(
      (s) => !s.heading.trim() || !s.description.trim(),
    );
    if (badSection) {
      await message("Every section needs a heading and instructions.", {
        kind: "warning",
      });
      return;
    }

    setSaving(true);
    try {
      const command = isNew
        ? "create_summary_template"
        : "update_summary_template";
      await invoke(command, { template });
      // Mark clean so the navigation triggered by onDone isn't blocked.
      initialRef.current = JSON.stringify(template);
      onDirtyChange(false);
      onDone();
    } catch (e) {
      await message(typeof e === "string" ? e : "Failed to save template.", {
        kind: "error",
      });
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="h-full overflow-auto">
      <div className="max-w-2xl mx-auto p-8 grid gap-6">
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={onDone}
            className="-ml-2"
            aria-label="Back to settings"
          >
            <ArrowLeft className="size-4" />
            Back
          </Button>
        </div>

        <div className="grid gap-1">
          <h2 className="text-lg font-semibold m-0">
            {isNew ? "New template" : `Edit ${template.name || "template"}`}
          </h2>
          {template.builtin && (
            <p className="text-xs text-muted-foreground">
              This is a built-in template. Your edits are saved separately and
              can be reset at any time.
            </p>
          )}
        </div>

        <div className="grid gap-2">
          <Label htmlFor="template-name">Name</Label>
          <Input
            id="template-name"
            value={template.name}
            placeholder="e.g. Sales call"
            onChange={(e) => patch({ name: e.target.value })}
          />
        </div>

        <div className="grid gap-2">
          <Label htmlFor="template-description">Description</Label>
          <Input
            id="template-description"
            value={template.description}
            placeholder="One line shown in the template picker."
            onChange={(e) => patch({ description: e.target.value })}
          />
        </div>

        <div className="grid gap-3">
          <div className="flex items-center justify-between">
            <Label className="text-sm font-medium">Sections</Label>
            <span className="text-xs text-muted-foreground">
              In order, as they appear in the summary
            </span>
          </div>

          {template.sections.map((section, index) => (
            <Card key={section.id} className="p-4 grid gap-3">
              <div className="flex items-center gap-2">
                <Input
                  value={section.heading}
                  placeholder="Section heading (e.g. Action items)"
                  className="font-medium"
                  onChange={(e) =>
                    patchSection(index, { heading: e.target.value })
                  }
                />
                <Button
                  variant="ghost"
                  size="icon"
                  disabled={index === 0}
                  onClick={() => moveSection(index, -1)}
                  aria-label="Move section up"
                >
                  <ArrowUp className="size-4" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  disabled={index === template.sections.length - 1}
                  onClick={() => moveSection(index, 1)}
                  aria-label="Move section down"
                >
                  <ArrowDown className="size-4" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  disabled={template.sections.length === 1}
                  onClick={() => removeSection(index)}
                  aria-label="Remove section"
                >
                  <Trash2 className="size-4" />
                </Button>
              </div>

              <div className="grid gap-1.5">
                <Label
                  htmlFor={`section-desc-${section.id}`}
                  className="text-xs text-muted-foreground"
                >
                  Instructions
                </Label>
                <Textarea
                  id={`section-desc-${section.id}`}
                  value={section.description}
                  rows={3}
                  placeholder="What this section should contain, and how to format it."
                  onChange={(e) =>
                    patchSection(index, { description: e.target.value })
                  }
                />
              </div>

              <div className="grid gap-1.5">
                <Label
                  htmlFor={`section-ex-${section.id}`}
                  className="text-xs text-muted-foreground"
                >
                  Example (optional)
                </Label>
                <Textarea
                  id={`section-ex-${section.id}`}
                  value={section.example ?? ""}
                  rows={2}
                  placeholder="An illustrative example of this section's output."
                  onChange={(e) =>
                    patchSection(index, { example: e.target.value })
                  }
                />
              </div>
            </Card>
          ))}

          <Button variant="outline" onClick={addSection} className="justify-self-start">
            <Plus className="size-4" />
            Add section
          </Button>
        </div>

        <div className="flex justify-end gap-2 pt-2">
          <Button variant="ghost" onClick={onDone} disabled={saving}>
            Cancel
          </Button>
          <Button onClick={() => void handleSave()} disabled={saving}>
            {saving && <Spinner className="size-4" />}
            {isNew ? "Create template" : "Save changes"}
          </Button>
        </div>
      </div>
    </div>
  );
}
