import { Bot, Crown, Shield, User } from "lucide-react";
import * as React from "react";

import {
  useAddChannelMembersMutation,
  useChannelMembersQuery,
  useRemoveChannelMemberMutation,
} from "@/features/channels/hooks";
import { useClassifiedMembers } from "@/features/channels/lib/useClassifiedMembers";
import { formatMemberName } from "@/features/channels/lib/memberUtils";
import { usePresenceQuery } from "@/features/presence/hooks";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import type { Channel, ChannelMember } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/shared/ui/sheet";
import { ChannelMemberInviteCard } from "./ChannelMemberInviteCard";

type MembersSidebarProps = {
  channel: Channel | null;
  currentPubkey?: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

function roleIcon(role: ChannelMember["role"]) {
  switch (role) {
    case "owner":
      return Crown;
    case "admin":
      return Shield;
    case "bot":
      return Bot;
    default:
      return User;
  }
}

export function MembersSidebar({
  channel,
  currentPubkey,
  open,
  onOpenChange,
}: MembersSidebarProps) {
  const channelId = channel?.id ?? null;
  const membersQuery = useChannelMembersQuery(channelId, open);
  const addMembersMutation = useAddChannelMembersMutation(channelId);
  const removeMemberMutation = useRemoveChannelMemberMutation(channelId);

  const rawMembers = membersQuery.data ?? [];
  const { people, bots, isBot } = useClassifiedMembers(
    rawMembers,
    currentPubkey,
  );

  const allMemberPubkeys = React.useMemo(
    () => rawMembers.map((member) => member.pubkey),
    [rawMembers],
  );
  const memberPresenceQuery = usePresenceQuery(allMemberPubkeys, {
    enabled: open && rawMembers.length > 0,
  });

  const selfMember =
    rawMembers.find((member) => member.pubkey === currentPubkey) ?? null;
  const canManageMembers =
    selfMember?.role === "owner" || selfMember?.role === "admin";
  const isArchived =
    channel?.archivedAt !== null && channel?.archivedAt !== undefined;

  if (!channel) {
    return null;
  }

  function renderMemberCard(member: ChannelMember, memberIsBot: boolean) {
    const Icon = memberIsBot ? Bot : roleIcon(member.role);

    return (
      <div
        className="flex items-start justify-between gap-3 rounded-2xl border border-border/80 bg-background px-4 py-3"
        data-testid={`sidebar-member-${member.pubkey}`}
        key={member.pubkey}
      >
        <div className="min-w-0 space-y-1">
          <div className="flex items-center gap-2">
            <Icon className="h-4 w-4 text-muted-foreground" />
            <p className="truncate text-sm font-medium">
              {formatMemberName(member, currentPubkey)}
            </p>
            {memberPresenceQuery.data ? (
              <PresenceBadge
                className="border-border/70 bg-muted/50 px-2 py-0.5 text-[10px] uppercase tracking-[0.14em]"
                data-testid={`sidebar-member-presence-${member.pubkey}`}
                status={
                  memberPresenceQuery.data[member.pubkey.toLowerCase()] ??
                  "offline"
                }
              />
            ) : null}
            <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              {memberIsBot ? "bot" : member.role}
            </span>
          </div>
        </div>
        {(selfMember?.role === "admin" && member.pubkey !== currentPubkey) ||
        (selfMember?.role === "owner" && isBot(member)) ||
        (currentPubkey && member.pubkey === currentPubkey) ? (
          <Button
            data-testid={`sidebar-remove-member-${member.pubkey}`}
            disabled={removeMemberMutation.isPending || isArchived}
            onClick={() => {
              void removeMemberMutation.mutateAsync(member.pubkey).then(() => {
                if (member.pubkey === currentPubkey) {
                  onOpenChange(false);
                }
              });
            }}
            size="sm"
            type="button"
            variant="ghost"
          >
            Remove
          </Button>
        ) : null}
      </div>
    );
  }

  return (
    <Sheet onOpenChange={onOpenChange} open={open}>
      <SheetContent
        className="flex w-full flex-col gap-0 overflow-hidden border-l border-border/80 bg-background p-0 sm:max-w-md"
        data-testid="members-sidebar"
        side="right"
      >
        <SheetHeader className="space-y-2 border-b border-border/80 bg-muted/20 px-6 py-6 text-left">
          <SheetTitle>Members</SheetTitle>
          <SheetDescription>
            People and bots in {channel.name}.
          </SheetDescription>
        </SheetHeader>

        <div className="flex-1 space-y-6 overflow-y-auto px-6 py-6">
          {(canManageMembers || channel.visibility === "open") &&
          channel.channelType !== "dm" ? (
            <ChannelMemberInviteCard
              existingMembers={rawMembers}
              isPending={addMembersMutation.isPending}
              onSubmit={(input) => addMembersMutation.mutateAsync(input)}
              open={open}
              requestErrorMessage={
                addMembersMutation.error instanceof Error
                  ? addMembersMutation.error.message
                  : null
              }
            />
          ) : null}

          <section className="space-y-3">
            <h2 className="text-sm font-semibold tracking-tight">
              People ({people.length})
            </h2>
            <div className="space-y-2" data-testid="members-sidebar-people">
              {people.length > 0 ? (
                people.map((member) => renderMemberCard(member, false))
              ) : (
                <p className="text-sm text-muted-foreground">
                  {membersQuery.isLoading
                    ? "Loading members..."
                    : "No people found."}
                </p>
              )}
            </div>
          </section>

          <section className="space-y-3">
            <h2 className="text-sm font-semibold tracking-tight">
              Bots ({bots.length})
            </h2>
            <div className="space-y-2" data-testid="members-sidebar-bots">
              {bots.length > 0 ? (
                bots.map((member) => renderMemberCard(member, true))
              ) : (
                <p className="text-sm text-muted-foreground">
                  {membersQuery.isLoading
                    ? "Loading members..."
                    : "No bots found."}
                </p>
              )}
            </div>
          </section>

          {removeMemberMutation.error instanceof Error ? (
            <p className="text-sm text-destructive">
              {removeMemberMutation.error.message}
            </p>
          ) : null}
        </div>
      </SheetContent>
    </Sheet>
  );
}
