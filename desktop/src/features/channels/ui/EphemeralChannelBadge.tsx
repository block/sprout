import { Clock } from "lucide-react";

import {
  EPHEMERAL_CHANNEL_LABEL,
  type EphemeralChannelDisplay,
} from "@/features/channels/lib/ephemeralChannel";
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
  const label =
    isHeader && display.detailLabel
      ? `${EPHEMERAL_CHANNEL_LABEL} · ${display.detailLabel}`
      : EPHEMERAL_CHANNEL_LABEL;

  return (
    <span
      {...accessibilityProps}
      className={cn(
        "inline-flex items-center gap-1 rounded-full font-medium text-amber-700 dark:text-amber-300",
        isHeader
          ? "border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-xs"
          : "shrink-0 h-4 w-4 justify-center border border-amber-500/20 bg-amber-500/10 p-0",
      )}
      data-testid={testId}
      title={display.tooltipLabel}
    >
      <Clock className={cn(isHeader ? "h-3 w-3" : "h-2.5 w-2.5")} />
      {isHeader ? <span>{label}</span> : null}
    </span>
  );
}
