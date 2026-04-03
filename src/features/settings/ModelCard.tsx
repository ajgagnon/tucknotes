import { Download } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { RadioGroupItem } from "@/components/ui/radio-group";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
  FieldTitle,
} from "@/components/ui/field";
import type { ModelInfo, DownloadProgress } from "@/features/models";
import { formatSize } from "@/features/models";
import { ModelActionsMenu } from "./ModelActionsMenu";

function DownloadProgressBar({
  progress,
  progressPercent,
}: {
  progress: DownloadProgress | null;
  progressPercent: number;
}) {
  return (
    <div className="mt-1.5">
      <div className="w-full bg-neutral-200 dark:bg-neutral-700 rounded-full h-1.5 mb-1 overflow-hidden">
        <div
          className="bg-primary h-full rounded-full transition-all duration-300 ease-out"
          style={{ width: `${progressPercent}%` }}
        />
      </div>
      <p className="text-xs text-muted-foreground tabular-nums">
        {progress
          ? `${formatSize(progress.downloaded_bytes)} / ${formatSize(progress.total_bytes)}`
          : "Starting download\u2026"}
      </p>
    </div>
  );
}

export function ModelCard({
  model,
  radioIdPrefix,
  isDownloaded,
  isDownloading,
  progress,
  progressPercent,
  anyDownloadInProgress,
  selectedId,
  onDownload,
  onRemove,
  onShowInFolder,
}: {
  model: ModelInfo;
  radioIdPrefix: string;
  isDownloaded: boolean;
  isDownloading: boolean;
  progress: DownloadProgress | null;
  progressPercent: number;
  anyDownloadInProgress: boolean;
  selectedId: string | null;
  onDownload: () => void;
  onRemove: () => void;
  onShowInFolder: () => void;
}) {
  if (!isDownloaded) {
    return (
      <div className="w-full rounded-lg border border-border p-2.5">
        <Field orientation="horizontal">
          <RadioGroupItem value={model.id} className="mt-0.5" />
          <FieldContent className="min-w-0">
            <FieldTitle>
              {model.name}
              {model.recommended && (
                <Badge variant="outline">Recommended</Badge>
              )}
            </FieldTitle>
            <FieldDescription>
              {model.description}
              <span className="text-muted-foreground/60">
                {" "}
                &middot; {formatSize(model.size_bytes)}
              </span>
            </FieldDescription>
            {isDownloading && (
              <DownloadProgressBar
                progress={progress}
                progressPercent={progressPercent}
              />
            )}
          </FieldContent>
          {!isDownloading && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="shrink-0"
              disabled={anyDownloadInProgress}
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                onDownload();
              }}
            >
              <Download className="size-3.5" />
              Download
            </Button>
          )}
        </Field>
      </div>
    );
  }

  const radioId = `${radioIdPrefix}-${model.id}`;
  return (
    <FieldLabel htmlFor={radioId} className="w-full">
      <Field orientation="horizontal" className="items-start">
        <RadioGroupItem value={model.id} id={radioId} className="mt-0.5" />
        <FieldContent>
          <FieldTitle>
            {model.name}
            {model.recommended && (
              <Badge variant="outline">Recommended</Badge>
            )}
          </FieldTitle>
          <FieldDescription>
            {model.description}
            <span className="text-muted-foreground/60">
              {" "}
              &middot; {formatSize(model.size_bytes)}
            </span>
          </FieldDescription>
        </FieldContent>
        <ModelActionsMenu
          canRemove={selectedId !== model.id}
          onShowInFolder={onShowInFolder}
          onRemove={onRemove}
        />
      </Field>
    </FieldLabel>
  );
}
