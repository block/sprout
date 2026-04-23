import { Download, RefreshCw } from "lucide-react";

import { Button } from "@/shared/ui/button";

import { useUpdaterContext } from "./hooks/UpdaterProvider";

const indicatorButtonClass =
  "relative h-6 w-6 text-muted-foreground/70 hover:bg-muted/60 hover:text-foreground";

export function UpdateIndicator({
  onOpenUpdates,
}: {
  onOpenUpdates: () => void;
}) {
  const { status } = useUpdaterContext();

  if (status.state === "available") {
    return (
      <Button
        aria-label="Update available"
        className={indicatorButtonClass}
        onClick={onOpenUpdates}
        size="icon"
        variant="ghost"
      >
        <Download className="h-3.5 w-3.5" />
        <span className="absolute -top-0.5 -right-0.5 h-2 w-2 rounded-full bg-primary animate-pulse" />
      </Button>
    );
  }

  if (status.state === "ready") {
    return (
      <Button
        aria-label="Restart to update"
        className={indicatorButtonClass}
        onClick={onOpenUpdates}
        size="icon"
        variant="ghost"
      >
        <RefreshCw className="h-3.5 w-3.5" />
        <span className="absolute -top-0.5 -right-0.5 h-2 w-2 rounded-full bg-emerald-500" />
      </Button>
    );
  }

  return null;
}
