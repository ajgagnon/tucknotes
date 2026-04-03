import { RadioGroup } from "@/components/ui/radio-group";
import { Skeleton } from "@/components/ui/skeleton";
import {
  useModelManager,
  type ModelManagerConfig,
} from "@/hooks/useModelManager";
import { cn } from "@/lib/utils";
import type { DownloadProgress } from "@/lib/models";
import { ModelCard } from "./ModelCard";

function progressPercentValue(p: DownloadProgress | null): number {
  if (!p || p.total_bytes <= 0) return 0;
  return Math.min((p.downloaded_bytes / p.total_bytes) * 100, 100);
}

export function ModelSection({
  title,
  config,
  radioIdPrefix,
  className,
}: {
  title: string;
  config: ModelManagerConfig;
  radioIdPrefix: string;
  className?: string;
}) {
  const {
    models,
    selectedId,
    downloadStatus,
    loading,
    downloading,
    progress,
    error,
    selectModel,
    downloadModel,
    removeModel,
    showInFolder,
  } = useModelManager(config);

  const pct = progressPercentValue(progress);

  return (
    <section className={cn(className)}>
      <h2 className="text-sm font-medium text-muted-foreground mb-4">
        {title}
      </h2>

      {error && !downloading && (
        <p className="text-sm text-destructive mb-2">{error}</p>
      )}

      {loading ? (
        <div className="flex flex-col gap-3">
          <Skeleton className="h-20 rounded-lg" />
          <Skeleton className="h-20 rounded-lg" />
        </div>
      ) : (
        <RadioGroup
          value={selectedId ?? undefined}
          onValueChange={(id) => void selectModel(id)}
          disabled={!!downloading}
          className="flex flex-col gap-2 w-full"
        >
          {models.map((model) => {
            const isDownloaded = downloadStatus[model.id] ?? false;
            const isDownloading = downloading === model.id;

            return (
              <ModelCard
                key={model.id}
                model={model}
                radioIdPrefix={radioIdPrefix}
                isDownloaded={isDownloaded}
                isDownloading={isDownloading}
                progress={isDownloading ? progress : null}
                progressPercent={isDownloading ? pct : 0}
                anyDownloadInProgress={!!downloading}
                selectedId={selectedId}
                onDownload={() => void downloadModel(model.id)}
                onRemove={() => void removeModel(model.id)}
                onShowInFolder={() => void showInFolder(model.id)}
              />
            );
          })}
        </RadioGroup>
      )}
    </section>
  );
}
