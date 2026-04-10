import * as React from "react";

import { cn } from "@/shared/lib/cn";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";

type UserAvatarSize = "xs" | "sm" | "md";

const sizeClasses: Record<UserAvatarSize, string> = {
  xs: "h-5 w-5 text-[8px]",
  sm: "h-6 w-6 text-[9px]",
  md: "h-8 w-8 text-[11px]",
};

type UserAvatarProps = {
  avatarUrl: string | null;
  displayName: string;
  size?: UserAvatarSize;
  accent?: boolean;
  className?: string;
};

export function UserAvatar({
  avatarUrl,
  displayName,
  size = "md",
  accent = false,
  className,
}: UserAvatarProps) {
  const [failedUrl, setFailedUrl] = React.useState<string | null>(null);
  const hasError = failedUrl === avatarUrl;

  const initials = displayName
    .split(" ")
    .map((part) => part[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();

  const base = cn(sizeClasses[size], "rounded-lg shadow-sm", className);

  if (avatarUrl && !hasError) {
    return (
      <img
        alt={`${displayName} avatar`}
        className={cn(base, "bg-secondary object-cover")}
        onError={() => setFailedUrl(avatarUrl)}
        referrerPolicy="no-referrer"
        src={rewriteRelayUrl(avatarUrl)}
      />
    );
  }

  return (
    <div
      className={cn(
        base,
        "flex items-center justify-center font-semibold",
        accent
          ? "bg-primary text-primary-foreground"
          : "bg-secondary text-secondary-foreground",
      )}
    >
      {initials}
    </div>
  );
}
