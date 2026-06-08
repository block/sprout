import { ArrowUpRight, Check, X } from "lucide-react";

import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";
import type { DispatchIntent } from "@/features/concierge/types";

type DispatchCardProps = {
  intent: DispatchIntent;
  onApprove: (id: string) => void;
  onDismiss: (id: string) => void;
};

/**
 * The one load-bearing new component: the `dispatch_agent` confirm card. The
 * Concierge proposes an action; nothing is sent until the human approves here.
 * The card is the safety boundary — the prompt is not.
 */
export function DispatchCard({
  intent,
  onApprove,
  onDismiss,
}: DispatchCardProps) {
  const settled = intent.status !== "pending";

  return (
    <div
      className={cn(
        "rounded-lg border border-primary/35 bg-primary/[0.06] px-4 py-3 transition-colors",
        intent.status === "approved" && "border-primary/50 bg-primary/[0.1]",
        intent.status === "dismissed" &&
          "border-border/60 bg-muted/30 opacity-60",
      )}
      data-status={intent.status}
      data-testid="dispatch-card"
    >
      <div className="flex items-center gap-1.5 text-xs font-medium uppercase tracking-wider text-primary/80">
        <ArrowUpRight className="h-3.5 w-3.5" />
        Dispatch
      </div>
      <p className="mt-1.5 text-sm text-foreground">
        <span className="font-semibold text-primary">@{intent.agent}</span>
        <span className="text-muted-foreground"> in </span>
        <span className="font-medium">#{intent.channel}</span>
      </p>
      <p className="mt-1 text-sm leading-snug text-muted-foreground">
        “{intent.instruction}”
      </p>

      {settled ? (
        <p className="mt-2.5 text-xs font-medium text-muted-foreground">
          {intent.status === "approved" ? "✓ Sent" : "Dismissed"}
        </p>
      ) : (
        <div className="mt-3 flex gap-2">
          <Button
            className="h-7 gap-1.5 px-3 text-xs"
            data-testid="dispatch-approve"
            onClick={() => onApprove(intent.id)}
            size="sm"
          >
            <Check className="h-3.5 w-3.5" />
            Approve
          </Button>
          <Button
            className="h-7 gap-1.5 px-3 text-xs"
            data-testid="dispatch-dismiss"
            onClick={() => onDismiss(intent.id)}
            size="sm"
            variant="ghost"
          >
            <X className="h-3.5 w-3.5" />
            Dismiss
          </Button>
        </div>
      )}
    </div>
  );
}
