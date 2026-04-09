import * as React from "react";

import {
  useAddChannelMembersMutation,
  useChannelMembersQuery,
  useRemoveChannelMemberMutation,
} from "@/features/channels/hooks";
import { useClassifiedMembers } from "@/features/channels/lib/useClassifiedMembers";
import { formatMemberName } from "@/features/channels/lib/memberUtils";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import { getPresenceLabel } from "@/features/presence/lib/presence";
import { usePresenceQuery } from "@/features/presence/hooks";
import { PresenceDot } from "@/features/presence/ui/PresenceBadge";
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

function formatRoleLabel(member: ChannelMember, memberIsBot: boolean) {
  if (memberIsBot) {
    return "Bot";
  }

  return `${member.role[0]?.toUpperCase() ?? ""}${member.role.slice(1)}`;
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
  const { people, bots, isBot, isMyBot } = useClassifiedMembers(
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
  const memberProfilesQuery = useUsersBatchQuery(allMemberPubkeys, {
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
    const canRemoveMember =
      (selfMember?.role === "admin" && member.pubkey !== currentPubkey) ||
      (selfMember?.role === "owner" && isBot(member)) ||
      isMyBot(member) ||
      (currentPubkey && member.pubkey === currentPubkey);
    const memberLabel = formatMemberName(member, currentPubkey);
    const profile =
      memberProfilesQuery.data?.profiles[member.pubkey.toLowerCase()] ?? null;
    const presenceStatus =
      memberPresenceQuery.data?.[member.pubkey.toLowerCase()] ?? null;
    const roleLabel = formatRoleLabel(member, memberIsBot);

    return (
      <div
        className="flex items-center justify-between gap-3 rounded-xl border border-border/80 bg-background px-3 py-2.5"
        data-testid={`sidebar-member-${member.pubkey}`}
        key={member.pubkey}
      >
        <div className="flex min-w-0 items-center gap-3">
          <ProfileAvatar
            avatarUrl={profile?.avatarUrl ?? null}
            className="h-9 w-9 rounded-full text-[11px] shadow-none"
            iconClassName="h-4 w-4"
            label={memberLabel}
          />
          <div className="min-w-0 space-y-0.5">
            <p className="truncate text-sm font-medium leading-5">
              {memberLabel}
            </p>
            <div
              className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground"
              data-testid={`sidebar-member-presence-${member.pubkey}`}
            >
              {presenceStatus ? (
                <>
                  <PresenceDot className="h-2 w-2" status={presenceStatus} />
                  <span>{getPresenceLabel(presenceStatus)}</span>
                  <span aria-hidden="true">&middot;</span>
                </>
              ) : null}
              <span>{roleLabel}</span>
            </div>
          </div>
        </div>
        {canRemoveMember ? (
          <Button
            className="h-8 shrink-0 rounded-full px-2.5 text-xs text-muted-foreground hover:text-foreground"
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

          <section className="space-y-2.5">
            <div className="flex items-center justify-between gap-2">
              <h2 className="text-sm font-semibold tracking-tight">People</h2>
              <span className="rounded-full bg-muted px-2 py-0.5 text-[11px] font-medium text-muted-foreground">
                {people.length}
              </span>
            </div>
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

          <section className="space-y-2.5">
            <div className="flex items-center justify-between gap-2">
              <h2 className="text-sm font-semibold tracking-tight">Bots</h2>
              <span className="rounded-full bg-muted px-2 py-0.5 text-[11px] font-medium text-muted-foreground">
                {bots.length}
              </span>
            </div>
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
