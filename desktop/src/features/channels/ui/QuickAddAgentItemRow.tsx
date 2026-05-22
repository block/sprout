import { Check, X } from "lucide-react";
import { motion } from "motion/react";
import * as React from "react";

import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { cn } from "@/shared/lib/cn";
import { Spinner } from "@/shared/ui/spinner";
import type { QuickAddAgentItem } from "./useQuickAddAgentItems";

// ── Avatar ────────────────────────────────────────────────────────────────────

export function QuickAddAgentAvatar({
  avatarUrl,
  label,
  isRunning,
}: {
  avatarUrl: string | null;
  label: string;
  isRunning: boolean;
}) {
  const initials = label
    .split(" ")
    .map((part) => part[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();

  return (
    <div className="relative flex h-7 w-7 shrink-0 items-center justify-center rounded-full border border-border/50 bg-muted/30">
      {avatarUrl ? (
        <img
          alt={label}
          className="h-full w-full rounded-full object-cover"
          referrerPolicy="no-referrer"
          src={rewriteRelayUrl(avatarUrl)}
        />
      ) : (
        <span className="text-[10px] font-semibold text-muted-foreground">
          {initials}
        </span>
      )}
      {isRunning ? (
        <span className="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-popover bg-emerald-500" />
      ) : null}
    </div>
  );
}

// ── Item Row ──────────────────────────────────────────────────────────────────

type QuickAddAgentItemRowProps = {
  item: QuickAddAgentItem;
  itemKey: string;
  isSelected: boolean;
  isPending: boolean;
  selectMode: boolean;
  disabled: boolean;
  onClick: () => void;
  onRemove: (pubkey: string) => void;
};

export const QuickAddAgentItemRow = React.memo(function QuickAddAgentItemRow({
  item,
  itemKey,
  isSelected,
  isPending,
  selectMode,
  disabled,
  onClick,
  onRemove,
}: QuickAddAgentItemRowProps) {
  const isInChannel = item.kind === "running-in-channel";

  return (
    <motion.div key={itemKey} layout transition={{ duration: 0.2 }}>
      <button
        aria-selected={isInChannel || isSelected}
        className={cn(
          "flex w-full items-center px-3 py-1.5 text-left text-sm transition-colors",
          isInChannel
            ? "cursor-default"
            : "cursor-pointer hover:bg-accent focus-visible:bg-accent focus-visible:outline-none",
          isPending && "pointer-events-none opacity-60",
          isSelected && "bg-accent/50",
        )}
        data-quick-add-item
        disabled={!isInChannel && disabled}
        onClick={onClick}
        role="option"
        tabIndex={0}
        type="button"
      >
        {!isInChannel ? (
          <motion.div
            animate={{
              width: selectMode ? 16 : 0,
              marginRight: selectMode ? 8 : 0,
              opacity: selectMode ? 1 : 0,
            }}
            initial={false}
            transition={{ duration: 0.15 }}
            className="shrink-0 overflow-hidden"
          >
            <div
              className={cn(
                "flex h-4 w-4 items-center justify-center rounded border transition-colors",
                isSelected
                  ? "border-primary bg-primary text-primary-foreground"
                  : "border-muted-foreground/40",
              )}
            >
              {isSelected ? <Check className="h-3 w-3" /> : null}
            </div>
          </motion.div>
        ) : null}
        <div className="flex min-w-0 flex-1 items-center gap-2.5">
          <span className={cn("shrink-0", isInChannel && "opacity-50")}>
            <QuickAddAgentAvatar
              avatarUrl={item.avatarUrl}
              label={item.label}
              isRunning={item.kind !== "persona"}
            />
          </span>
          <span
            className={cn(
              "min-w-0 flex-1 truncate font-medium",
              isInChannel && "opacity-50",
            )}
          >
            {item.label}
          </span>
          {isInChannel ? (
            <button
              className="flex h-5 w-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
              onClick={(e) => {
                e.stopPropagation();
                onRemove(item.agent.pubkey);
              }}
              type="button"
              aria-label={`Remove ${item.label}`}
            >
              <X className="h-3.5 w-3.5" />
            </button>
          ) : null}
          {isPending ? (
            <Spinner className="h-3.5 w-3.5 shrink-0 text-primary" />
          ) : null}
        </div>
      </button>
    </motion.div>
  );
});
