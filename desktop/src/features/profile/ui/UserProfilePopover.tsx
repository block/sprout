import * as React from "react";

import { useUserProfileQuery } from "@/features/profile/hooks";
import { usePresenceQuery } from "@/features/presence/hooks";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";

type UserProfilePopoverProps = {
  children: React.ReactNode;
  pubkey: string;
};

function truncatePubkey(pubkey: string) {
  if (pubkey.length <= 16) {
    return pubkey;
  }

  return `${pubkey.slice(0, 8)}…${pubkey.slice(-8)}`;
}

export function UserProfilePopover({
  children,
  pubkey,
}: UserProfilePopoverProps) {
  const [open, setOpen] = React.useState(false);
  const profileQuery = useUserProfileQuery(open ? pubkey : undefined);
  const presenceQuery = usePresenceQuery(open ? [pubkey] : [], {
    enabled: open,
  });

  const profile = profileQuery.data;
  const presenceStatus = presenceQuery.data?.[pubkey.toLowerCase()];

  return (
    <Popover onOpenChange={setOpen} open={open}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent align="start" className="w-72" side="top" sideOffset={8}>
        <div className="flex flex-col gap-3">
          <div className="flex items-start gap-3">
            {profile?.avatarUrl ? (
              <img
                alt={profile.displayName ?? "User avatar"}
                className="h-10 w-10 shrink-0 rounded-xl object-cover shadow-sm"
                referrerPolicy="no-referrer"
                src={profile.avatarUrl}
              />
            ) : (
              <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-secondary text-xs font-semibold text-secondary-foreground shadow-sm">
                {(profile?.displayName ?? pubkey.slice(0, 2))
                  .slice(0, 2)
                  .toUpperCase()}
              </div>
            )}

            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-semibold">
                {profile?.displayName ?? truncatePubkey(pubkey)}
              </p>
              {profile?.nip05Handle ? (
                <p className="truncate text-xs text-muted-foreground">
                  {profile.nip05Handle}
                </p>
              ) : null}
            </div>

            {presenceStatus ? <PresenceBadge status={presenceStatus} /> : null}
          </div>

          {profile?.about ? (
            <p className="text-xs leading-relaxed text-muted-foreground">
              {profile.about}
            </p>
          ) : null}

          <p className="truncate font-mono text-[10px] text-muted-foreground/60">
            {truncatePubkey(pubkey)}
          </p>
        </div>
      </PopoverContent>
    </Popover>
  );
}
