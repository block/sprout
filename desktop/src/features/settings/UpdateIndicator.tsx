import { Download, Loader2, RefreshCw } from "lucide-react";

import { Button } from "@/shared/ui/button";

import { useUpdaterContext } from "./hooks/UpdaterProvider";
import type { UpdateStatus } from "./hooks/use-updater";

const indicatorButtonClass =
  "relative text-muted-foreground/70 hover:bg-muted/60 hover:text-foreground";

const iconClass = "h-3.5 w-3.5";

const variants: Record<
  "available" | "downloading" | "installing" | "ready",
  {
    Icon: typeof Download;
    label: string;
    badgeColor: string;
    iconClass: string;
  }
> = {
  available: {
    Icon: Download,
    label: "Update available",
    badgeColor: "bg-primary",
    iconClass: iconClass,
  },
  downloading: {
    Icon: Loader2,
    label: "Downloading update\u2026",
    badgeColor: "bg-primary",
    iconClass: `${iconClass} animate-spin`,
  },
  installing: {
    Icon: Loader2,
    label: "Installing update\u2026",
    badgeColor: "bg-primary",
    iconClass: `${iconClass} animate-spin`,
  },
  ready: {
    Icon: RefreshCw,
    label: "Restart to update",
    badgeColor: "bg-emerald-500",
    iconClass: iconClass,
  },
};

function getVariant(state: UpdateStatus["state"]) {
  if (
    state === "available" ||
    state === "downloading" ||
    state === "installing" ||
    state === "ready"
  ) {
    return variants[state];
  }
  return null;
}

export function UpdateIndicator({
  onOpenUpdates,
}: {
  onOpenUpdates: () => void;
}) {
  const { status } = useUpdaterContext();
  const variant = getVariant(status.state);

  if (!variant) {
    return null;
  }

  const { Icon, label, badgeColor, iconClass: variantIconClass } = variant;

  return (
    <Button
      aria-label={label}
      className={indicatorButtonClass}
      onClick={onOpenUpdates}
      size="sm"
      variant="ghost"
    >
      <Icon className={variantIconClass} />
      {label}
      <span
        className={`absolute -top-0.5 -right-0.5 h-2 w-2 rounded-full ${badgeColor} animate-pulse`}
      />
    </Button>
  );
}
