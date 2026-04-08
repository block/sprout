import type { CheckedState } from "@radix-ui/react-checkbox";

import type { AgentPersona } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Checkbox } from "@/shared/ui/checkbox";

import { getPersonaCatalogToggleAriaLabel } from "./PersonaCatalogSelectionBadge";
import { personaCatalogCopy } from "./personaLibraryCopy";

type PersonaCatalogSelectionControlProps = {
  isPending: boolean;
  onCheckedChange: (checked: CheckedState) => void;
  persona: AgentPersona;
  variant: "card" | "detail";
};

export function PersonaCatalogSelectionControl({
  isPending,
  onCheckedChange,
  persona,
  variant,
}: PersonaCatalogSelectionControlProps) {
  const prefix =
    variant === "card"
      ? "persona-catalog-toggle"
      : "persona-catalog-detail-toggle";
  const controlId = `${prefix}-control-${persona.id}`;
  const labelClassName =
    variant === "card"
      ? cn(
          "flex items-center gap-2 rounded-md px-2 py-1 text-xs font-medium text-foreground transition-colors",
          isPending
            ? "cursor-not-allowed opacity-70"
            : "cursor-pointer hover:bg-muted/60",
        )
      : "flex cursor-pointer items-center gap-2 rounded-md px-2 py-1 text-xs font-medium text-foreground transition-colors hover:bg-muted/60";

  return (
    <label
      className={labelClassName}
      data-testid={`${prefix}-target-${persona.id}`}
      htmlFor={controlId}
    >
      <Checkbox
        aria-label={getPersonaCatalogToggleAriaLabel(persona.displayName)}
        checked={persona.isActive}
        data-testid={`${prefix}-${persona.id}`}
        disabled={isPending}
        id={controlId}
        onCheckedChange={onCheckedChange}
      />
      <span>
        {persona.isActive
          ? personaCatalogCopy.deselectAction
          : personaCatalogCopy.selectAction}
      </span>
    </label>
  );
}
