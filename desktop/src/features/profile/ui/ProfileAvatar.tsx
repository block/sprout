import { UserRound } from "lucide-react";
import * as React from "react";

import { cn } from "@/shared/lib/cn";

type ProfileAvatarProps = {
  avatarUrl: string | null;
  label: string;
  className?: string;
  iconClassName?: string;
  testId?: string;
};

export function ProfileAvatar({
  avatarUrl,
  label,
  className,
  iconClassName,
  testId,
}: ProfileAvatarProps) {
  const [failedAvatarUrl, setFailedAvatarUrl] = React.useState<string | null>(
    null,
  );

  const initials = label
    .trim()
    .split(/\s+/)
    .map((part) => part[0] ?? "")
    .join("")
    .slice(0, 2)
    .toUpperCase();
  const baseClassName = cn(
    "flex shrink-0 items-center justify-center overflow-hidden border border-border/80 bg-primary/10 text-primary shadow-sm",
    className,
  );

  if (avatarUrl && failedAvatarUrl !== avatarUrl) {
    return (
      <img
        alt={`${label} avatar`}
        className={cn(baseClassName, "object-cover")}
        data-testid={testId}
        onError={() => {
          setFailedAvatarUrl(avatarUrl);
        }}
        referrerPolicy="no-referrer"
        src={avatarUrl}
      />
    );
  }

  return (
    <div className={cn(baseClassName, "font-semibold")} data-testid={testId}>
      {initials.length > 0 ? initials : <UserRound className={iconClassName} />}
    </div>
  );
}
