import { Clock } from "lucide-react";

import type { EphemeralChannelDisplay } from "@/features/channels/lib/ephemeralChannel";
import { cn } from "@/shared/lib/cn";

type EphemeralChannelBadgeProps = {
  display: EphemeralChannelDisplay;
  testId?: string;
  variant: "header" | "sidebar";
};

export function EphemeralChannelBadge({
  display,
  testId,
  variant,
}: EphemeralChannelBadgeProps) {
  const isHeader = variant === "header";
  const accessibilityProps = isHeader
    ? {}
    : {
        "aria-label": display.tooltipLabel,
        role: "img" as const,
      };

  return (
    <span
      {...accessibilityProps}
      className={cn(
        "inline-flex items-center gap-1 rounded-full font-medium text-slate-500 dark:text-slate-400",
        isHeader
          ? "h-5 w-5 justify-center border border-sky-500/20 bg-sky-500/5 p-0 text-xs"
          : "shrink-0 h-4 w-4 justify-center border border-sky-500/15 bg-slate-500/5 p-0 text-slate-500/80 dark:text-slate-400/80",
      )}
      data-testid={testId}
      title={display.tooltipLabel}
    >
      <Clock className={cn(isHeader ? "h-3 w-3" : "h-2.5 w-2.5")} />
    </span>
  );
}
