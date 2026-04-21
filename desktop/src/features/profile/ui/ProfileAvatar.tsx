import { UserRound } from "lucide-react";

import { cn } from "@/shared/lib/cn";
import { getInitials } from "@/shared/lib/initials";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { Avatar, AvatarFallback, AvatarImage } from "@/shared/ui/avatar";

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
  const initials = getInitials(label);

  return (
    <Avatar
      className={cn("shrink-0 bg-primary/20 text-primary shadow-sm", className)}
      data-testid={testId}
    >
      {avatarUrl ? (
        <AvatarImage
          alt={`${label} avatar`}
          className="object-cover"
          referrerPolicy="no-referrer"
          src={rewriteRelayUrl(avatarUrl)}
        />
      ) : null}
      <AvatarFallback
        className="bg-primary/20 font-semibold text-primary"
        delayMs={200}
      >
        {initials.length > 0 ? (
          initials
        ) : (
          <UserRound className={iconClassName} />
        )}
      </AvatarFallback>
    </Avatar>
  );
}
