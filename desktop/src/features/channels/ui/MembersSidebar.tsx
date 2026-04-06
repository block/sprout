import { Bot, Crown, Shield, User } from "lucide-react";
import * as React from "react";

import {
  useManagedAgentsQuery,
  useRelayAgentsQuery,
} from "@/features/agents/hooks";
import {
  useAddChannelMembersMutation,
  useChannelMembersQuery,
  useRemoveChannelMemberMutation,
} from "@/features/channels/hooks";
import { usePresenceQuery } from "@/features/presence/hooks";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import type { Channel, ChannelMember } from "@/shared/api/types";
import { normalizePubkey } from "@/shared/lib/pubkey";
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

const roleOrder: Record<ChannelMember["role"], number> = {
  owner: 0,
  admin: 1,
  member: 2,
  guest: 3,
  bot: 4,
};

function formatPubkey(pubkey: string) {
  return `${pubkey.slice(0, 8)}...${pubkey.slice(-4)}`;
}

function formatMemberName(member: ChannelMember, currentPubkey?: string) {
  if (currentPubkey && member.pubkey === currentPubkey) {
    return "You";
  }

  return member.displayName ?? formatPubkey(member.pubkey);
}

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
  const managedAgentsQuery = useManagedAgentsQuery();
  const relayAgentsQuery = useRelayAgentsQuery();
  const addMembersMutation = useAddChannelMembersMutation(channelId);
  const removeMemberMutation = useRemoveChannelMemberMutation(channelId);

  const managedAgents = managedAgentsQuery.data ?? [];
  const relayAgents = relayAgentsQuery.data ?? [];
  const rawMembers = membersQuery.data ?? [];

  const managedAgentPubkeys = React.useMemo(
    () => new Set(managedAgents.map((agent) => normalizePubkey(agent.pubkey))),
    [managedAgents],
  );
  const relayAgentPubkeys = React.useMemo(
    () => new Set(relayAgents.map((agent) => normalizePubkey(agent.pubkey))),
    [relayAgents],
  );

  const { people, bots } = React.useMemo(() => {
    const peopleList: ChannelMember[] = [];
    const botList: ChannelMember[] = [];

    for (const member of rawMembers) {
      const normalizedPubkey = normalizePubkey(member.pubkey);
      if (
        member.role === "bot" ||
        managedAgentPubkeys.has(normalizedPubkey) ||
        relayAgentPubkeys.has(normalizedPubkey)
      ) {
        botList.push(member);
      } else {
        peopleList.push(member);
      }
    }

    const sortMembers = (list: ChannelMember[]) =>
      [...list].sort((left, right) => {
        if (currentPubkey && left.pubkey === currentPubkey) {
          return -1;
        }
        if (currentPubkey && right.pubkey === currentPubkey) {
          return 1;
        }
        const roleDelta = roleOrder[left.role] - roleOrder[right.role];
        if (roleDelta !== 0) {
          return roleDelta;
        }
        return formatMemberName(left).localeCompare(formatMemberName(right));
      });

    return { people: sortMembers(peopleList), bots: sortMembers(botList) };
  }, [currentPubkey, managedAgentPubkeys, rawMembers, relayAgentPubkeys]);

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

  function renderMemberCard(member: ChannelMember, isBot: boolean) {
    const Icon = isBot ? Bot : roleIcon(member.role);

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
              {isBot ? "bot" : member.role}
            </span>
          </div>
        </div>
        {canManageMembers && member.pubkey !== currentPubkey ? (
          <Button
            data-testid={`sidebar-remove-member-${member.pubkey}`}
            disabled={removeMemberMutation.isPending || isArchived}
            onClick={() => {
              void removeMemberMutation.mutateAsync(member.pubkey);
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
          {canManageMembers && channel.channelType !== "dm" ? (
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
