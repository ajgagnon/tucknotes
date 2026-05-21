"use client";

import {
  createContext,
  useContext,
  useMemo,
  type ComponentProps,
  type ReactNode,
} from "react";

import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from "@/components/ui/hover-card";
import { cn } from "@/lib/utils";

export type ContextUsage = {
  inputTokens?: number;
  outputTokens?: number;
  reasoningTokens?: number;
  cacheTokens?: number;
};

type ContextValue = {
  usedTokens: number;
  maxTokens: number;
  usage?: ContextUsage;
  modelId?: string;
  percent: number;
};

const ContextValueCtx = createContext<ContextValue | null>(null);

const useContextValue = () => {
  const ctx = useContext(ContextValueCtx);
  if (!ctx) {
    throw new Error(
      "Context.* sub-components must be rendered inside <Context>",
    );
  }
  return ctx;
};

const clampPercent = (used: number, max: number) => {
  if (!Number.isFinite(used) || !Number.isFinite(max) || max <= 0) return 0;
  return Math.max(0, Math.min(100, (used / max) * 100));
};

const formatTokens = (n: number) =>
  Number.isFinite(n) ? n.toLocaleString() : "—";

export type ContextProps = ComponentProps<typeof HoverCard> & {
  usedTokens: number;
  maxTokens: number;
  usage?: ContextUsage;
  modelId?: string;
};

export const Context = ({
  usedTokens,
  maxTokens,
  usage,
  modelId,
  children,
  ...props
}: ContextProps) => {
  const value = useMemo<ContextValue>(
    () => ({
      usedTokens,
      maxTokens,
      usage,
      modelId,
      percent: clampPercent(usedTokens, maxTokens),
    }),
    [usedTokens, maxTokens, usage, modelId],
  );
  return (
    <ContextValueCtx.Provider value={value}>
      <HoverCard {...props}>{children}</HoverCard>
    </ContextValueCtx.Provider>
  );
};

export type ContextTriggerProps = ComponentProps<typeof HoverCardTrigger>;

export const ContextTrigger = ({
  className,
  children,
  ...props
}: ContextTriggerProps) => {
  const { percent, usedTokens, maxTokens } = useContextValue();
  const pct = Math.round(percent);
  return (
    <HoverCardTrigger
      data-slot="context-trigger"
      className={cn(
        "inline-flex h-7 items-center gap-1.5 rounded-md px-1.5 text-xs text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring",
        className,
      )}
      aria-label={`Context: ${formatTokens(usedTokens)} of ${formatTokens(maxTokens)} tokens used (${pct}%)`}
      {...props}
    >
      {children ?? (
        <>
          <ContextRing percent={percent} />
          <span className="tabular-nums">{pct}%</span>
        </>
      )}
    </HoverCardTrigger>
  );
};

const ContextRing = ({ percent }: { percent: number }) => {
  const radius = 6;
  const circumference = 2 * Math.PI * radius;
  const dash = (percent / 100) * circumference;
  return (
    <svg
      viewBox="0 0 16 16"
      className="size-3.5 shrink-0"
      aria-hidden="true"
      role="img"
    >
      <circle
        cx="8"
        cy="8"
        r={radius}
        fill="none"
        stroke="currentColor"
        strokeOpacity={0.2}
        strokeWidth={2}
      />
      <circle
        cx="8"
        cy="8"
        r={radius}
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinecap="round"
        strokeDasharray={`${dash} ${circumference - dash}`}
        transform="rotate(-90 8 8)"
      />
    </svg>
  );
};

export type ContextContentProps = ComponentProps<typeof HoverCardContent>;

export const ContextContent = ({
  className,
  side = "top",
  align = "end",
  ...props
}: ContextContentProps) => (
  <HoverCardContent
    data-slot="context-content"
    side={side}
    align={align}
    className={cn("w-64 text-xs", className)}
    {...props}
  />
);

export type ContextContentHeaderProps = ComponentProps<"div">;

export const ContextContentHeader = ({
  className,
  children,
  ...props
}: ContextContentHeaderProps) => {
  const { usedTokens, maxTokens, percent } = useContextValue();
  return (
    <div
      data-slot="context-content-header"
      className={cn("flex flex-col gap-1.5", className)}
      {...props}
    >
      {children ?? (
        <>
          <div className="flex items-center justify-between gap-2">
            <span className="font-medium text-foreground">Context</span>
            <span className="tabular-nums text-muted-foreground">
              {formatTokens(usedTokens)} / {formatTokens(maxTokens)} (
              {Math.round(percent)}%)
            </span>
          </div>
          <div className="h-1 w-full overflow-hidden rounded-full bg-muted">
            <div
              className="h-full rounded-full bg-foreground/70"
              style={{ width: `${percent}%` }}
            />
          </div>
        </>
      )}
    </div>
  );
};

export type ContextContentBodyProps = ComponentProps<"div">;

export const ContextContentBody = ({
  className,
  ...props
}: ContextContentBodyProps) => (
  <div
    data-slot="context-content-body"
    className={cn("mt-2.5 flex flex-col gap-1", className)}
    {...props}
  />
);

export type ContextContentFooterProps = ComponentProps<"div">;

export const ContextContentFooter = ({
  className,
  children,
  ...props
}: ContextContentFooterProps) => (
  <div
    data-slot="context-content-footer"
    className={cn(
      "mt-2 border-t border-border pt-2 text-muted-foreground",
      className,
    )}
    {...props}
  >
    {children}
  </div>
);

type UsageRowProps = ComponentProps<"div"> & {
  label: string;
  tokens: number | undefined;
};

const UsageRow = ({
  label,
  tokens,
  className,
  children,
  ...props
}: UsageRowProps) => {
  if (tokens === undefined) return null;
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-2 text-muted-foreground",
        className,
      )}
      {...props}
    >
      <span>{label}</span>
      {children ?? (
        <span className="tabular-nums text-foreground">
          {formatTokens(tokens)} tokens
        </span>
      )}
    </div>
  );
};

export type ContextUsageRowProps = Omit<ComponentProps<"div">, "children"> & {
  children?: ReactNode;
};

export const ContextInputUsage = ({ ...props }: ContextUsageRowProps) => {
  const { usage } = useContextValue();
  return (
    <UsageRow
      data-slot="context-input-usage"
      label="Input"
      tokens={usage?.inputTokens}
      {...props}
    />
  );
};

export const ContextOutputUsage = ({ ...props }: ContextUsageRowProps) => {
  const { usage } = useContextValue();
  return (
    <UsageRow
      data-slot="context-output-usage"
      label="Output"
      tokens={usage?.outputTokens}
      {...props}
    />
  );
};

export const ContextReasoningUsage = ({ ...props }: ContextUsageRowProps) => {
  const { usage } = useContextValue();
  return (
    <UsageRow
      data-slot="context-reasoning-usage"
      label="Reasoning"
      tokens={usage?.reasoningTokens}
      {...props}
    />
  );
};

export const ContextCacheUsage = ({ ...props }: ContextUsageRowProps) => {
  const { usage } = useContextValue();
  return (
    <UsageRow
      data-slot="context-cache-usage"
      label="Cache"
      tokens={usage?.cacheTokens}
      {...props}
    />
  );
};
