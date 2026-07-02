import { ListRestart } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { SETTINGS_SECTION_TEMPLATES } from "@/features/settings/TemplateSection";
import type { TemplateInfo } from "./types";

/// Sentinel value for the "Edit templates…" action in the template Select.
const EDIT_TEMPLATES_VALUE = "__edit_templates__";

/** Footer bar on the summary tab: template picker + (re)summarize button. */
export function SummarizeFooter({
  templates,
  selectedTemplate,
  onTemplateChange,
  onSummarize,
  hasSummary,
  onOpenSettings,
}: {
  templates: TemplateInfo[];
  selectedTemplate: string;
  onTemplateChange: (templateId: string) => void;
  onSummarize: () => void;
  hasSummary: boolean;
  onOpenSettings?: (section?: string) => void;
}) {
  return (
    <div className="mt-0 shrink-0 flex flex-row items-center gap-2 border-t px-4 py-3">
      {templates.length > 0 && (
        <div className="flex items-center gap-1.5">
          <Select
            value={selectedTemplate}
            onValueChange={(value) => {
              if (value === EDIT_TEMPLATES_VALUE) {
                onOpenSettings?.(SETTINGS_SECTION_TEMPLATES);
                return;
              }
              onTemplateChange(value as string);
            }}
          >
            <SelectTrigger size="sm" aria-label="Summary template">
              <SelectValue>
                {(value: string | null) =>
                  templates.find((t) => t.id === value)?.name ?? "Recap"
                }
              </SelectValue>
            </SelectTrigger>
            <SelectContent className="min-w-52">
              <SelectGroup>
                <SelectLabel>Templates</SelectLabel>
                {templates.map((t) => (
                  <SelectItem key={t.id} value={t.id}>
                    {t.name}
                  </SelectItem>
                ))}
              </SelectGroup>
              {onOpenSettings && (
                <SelectGroup>
                  <SelectSeparator />
                  <SelectItem value={EDIT_TEMPLATES_VALUE}>
                    Edit templates…
                  </SelectItem>
                </SelectGroup>
              )}
            </SelectContent>
          </Select>
        </div>
      )}

      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={hasSummary ? "Resummarize" : "Summarize"}
              onClick={onSummarize}
            />
          }
        >
          <ListRestart className="size-4" />
        </TooltipTrigger>
        <TooltipContent>
          {hasSummary ? "Resummarize" : "Summarize"}
        </TooltipContent>
      </Tooltip>
    </div>
  );
}
