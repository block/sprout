import type { ReactNode } from "react";

import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";

type MembersSidebarIconButtonProps = {
  actionLabel: string;
  className?: string;
  "data-testid": string;
  disabled: boolean;
  icon: ReactNode;
  onClick: () => void;
  variant: "default" | "ghost" | "outline";
};

export function MembersSidebarIconButton({
  actionLabel,
  className,
  "data-testid": testId,
  disabled,
  icon,
  onClick,
  variant,
}: MembersSidebarIconButtonProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="inline-flex">
          <Button
            aria-label={actionLabel}
            className={cn("h-8 w-8 rounded-full", className)}
            data-testid={testId}
            disabled={disabled}
            onClick={onClick}
            size="icon"
            type="button"
            variant={variant}
          >
            {icon}
          </Button>
        </span>
      </TooltipTrigger>
      <TooltipContent>{actionLabel}</TooltipContent>
    </Tooltip>
  );
}
