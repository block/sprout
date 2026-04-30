import { Loader2 } from "lucide-react";

import { cn } from "@/shared/lib/cn";

type SpinnerProps = React.ComponentPropsWithoutRef<"svg"> & {
  className?: string;
  size?: number;
};

export function Spinner({
  className,
  size,
  role = "status",
  "aria-label": ariaLabel = "Loading",
  ...rest
}: SpinnerProps) {
  return (
    <Loader2
      className={cn("animate-spin", className)}
      size={size}
      role={role}
      aria-label={ariaLabel}
      {...rest}
    />
  );
}
