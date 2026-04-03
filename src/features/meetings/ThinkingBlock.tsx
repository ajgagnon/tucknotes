import { AnimatedShinyText } from "@/components/ui/animated-shiny-text";

export function ThinkingBlock({ text }: { text: string }) {
  const lines = text.trimEnd().split("\n");
  const lastLine = lines.length > 0 ? lines[lines.length - 1]! : "";

  return (
    <div className="h-5 overflow-hidden w-0 min-w-full text-sm text-muted-foreground italic">
      <AnimatedShinyText className="truncate m-0">{lastLine}</AnimatedShinyText>
    </div>
  );
}
