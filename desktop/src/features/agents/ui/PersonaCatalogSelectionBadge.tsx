import { cn } from "@/shared/lib/cn";

import { personaCatalogCopy } from "./personaLibraryCopy";

type PersonaCatalogSelectionBadgeProps = {
  isActive: boolean;
};

export function PersonaCatalogSelectionBadge({
  isActive,
}: PersonaCatalogSelectionBadgeProps) {
  return (
    <span
      className={cn(
        "whitespace-nowrap rounded-full px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.14em]",
        isActive
          ? "bg-primary/15 text-primary"
          : "bg-muted text-muted-foreground",
      )}
    >
      {isActive
        ? personaCatalogCopy.selectedState
        : personaCatalogCopy.availableState}
    </span>
  );
}
