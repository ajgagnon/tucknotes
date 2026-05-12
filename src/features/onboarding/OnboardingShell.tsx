import type { ReactNode } from "react";

interface OnboardingShellProps {
  icon?: ReactNode;
  title: string;
  description?: ReactNode;
  step?: { index: number; total: number };
  children: ReactNode;
}

function OnboardingShell({
  icon,
  title,
  description,
  step,
  children,
}: OnboardingShellProps) {
  return (
    <div className="min-h-screen flex items-center justify-center px-6 py-12 bg-canvas">
      <div className="w-full max-w-md flex flex-col items-center text-center gap-6">
        {icon && (
          <div className="size-16 rounded-full bg-primary/10 text-primary flex items-center justify-center [&_svg]:size-7">
            {icon}
          </div>
        )}

        <div className="flex flex-col gap-2">
          <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
          {description && (
            <p className="text-sm text-muted-foreground leading-relaxed max-w-sm mx-auto">
              {description}
            </p>
          )}
        </div>

        {children}

        {step && (
          <div
            className="flex items-center gap-2 pt-2"
            role="progressbar"
            aria-valuemin={1}
            aria-valuemax={step.total}
            aria-valuenow={step.index + 1}
            aria-label={`Step ${step.index + 1} of ${step.total}`}
          >
            {Array.from({ length: step.total }).map((_, i) => (
              <span
                key={i}
                className={`size-1.5 rounded-full transition-colors ${
                  i === step.index ? "bg-primary" : "bg-border"
                }`}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export default OnboardingShell;
