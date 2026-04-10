import * as React from "react";
import {
  useAddChannelMembersMutation,
  useChannelMembersQuery,
} from "@/features/channels/hooks";
import { useClassifiedMembers } from "@/features/channels/lib/useClassifiedMembers";
import { formatMemberName } from "@/features/channels/lib/memberUtils";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import { usePresenceQuery } from "@/features/presence/hooks";
import type { Channel, ChannelMember } from "@/shared/api/types";
import { normalizePubkey } from "@/shared/lib/pubkey";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/shared/ui/sheet";
import { MembersSidebarAgentControls } from "./MembersSidebarAgentControls";
import { ChannelMemberInviteCard } from "./ChannelMemberInviteCard";
import { MembersSidebarMemberCard } from "./MembersSidebarMemberCard";
import { useMembersSidebarActions } from "./useMembersSidebarActions";

type MembersSidebarProps = {
  channel: Channel | null;
  currentPubkey?: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function MembersSidebar({
  channel,
  currentPubkey,
  open,
  onOpenChange,
}: MembersSidebarProps) {
  const channelId = channel?.id ?? null;
  const membersQuery = useChannelMembersQuery(channelId, open);
  const addMembersMutation = useAddChannelMembersMutation(channelId);

  const rawMembers = membersQuery.data ?? [];
  const { people, bots, isBot, isMyBot, managedAgentsQuery } =
    useClassifiedMembers(rawMembers, currentPubkey);

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
  const managedAgentByPubkey = React.useMemo(
    () =>
      new Map(
        (managedAgentsQuery.data ?? []).map((agent) => [
          normalizePubkey(agent.pubkey),
          agent,
        ]),
      ),
    [managedAgentsQuery.data],
  );
  const controllableManagedBots = React.useMemo(
    () =>
      bots.flatMap((member) => {
        const agent = managedAgentByPubkey.get(normalizePubkey(member.pubkey));
        return agent ? [agent] : [];
      }),
    [bots, managedAgentByPubkey],
  );
  const canRemoveMember = React.useCallback(
    (member: ChannelMember) => {
      return (
        (selfMember?.role === "admin" && member.pubkey !== currentPubkey) ||
        (selfMember?.role === "owner" && isBot(member)) ||
        Boolean(selfMember && isMyBot(member)) ||
        member.pubkey === currentPubkey
      );
    },
    [currentPubkey, isBot, isMyBot, selfMember],
  );
  const removableManagedBots = React.useMemo(
    () =>
      bots.flatMap((member) => {
        if (!canRemoveMember(member)) {
          return [];
        }

        const agent = managedAgentByPubkey.get(normalizePubkey(member.pubkey));
        return agent ? [agent] : [];
      }),
    [bots, canRemoveMember, managedAgentByPubkey],
  );
  const {
    actionErrorMessage,
    actionNoticeMessage,
    handleLifecycleAction: handleAgentLifecycleAction,
    handleRemoveAll,
    handleRemoveMember,
    handleRespawnAll,
    handleStopAll,
    hasControllableManagedBots,
    hasRemovableManagedBots,
    hasStoppableManagedBots,
    isActionPending,
  } = useMembersSidebarActions({
    channelId,
    controllableManagedBots,
    removableManagedBots,
    currentPubkey,
    onOpenChange,
  });

  if (!channel) {
    return null;
  }

  function renderMemberCard(member: ChannelMember, memberIsBot: boolean) {
    return (
      <MembersSidebarMemberCard
        canRemoveMember={canRemoveMember(member)}
        isActionPending={isActionPending}
        isArchived={isArchived}
        key={member.pubkey}
        managedAgent={
          memberIsBot
            ? managedAgentByPubkey.get(normalizePubkey(member.pubkey))
            : undefined
        }
        member={member}
        memberIsBot={memberIsBot}
        memberLabel={formatMemberName(member, currentPubkey)}
        onManagedAgentAction={(agent) => {
          void handleAgentLifecycleAction(agent);
        }}
        onRemoveMember={handleRemoveMember}
        presenceStatus={
          memberPresenceQuery.data?.[member.pubkey.toLowerCase()] ?? null
        }
        profileAvatarUrl={
          memberProfilesQuery.data?.profiles[member.pubkey.toLowerCase()]
            ?.avatarUrl ?? null
        }
      />
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
            <div className="flex items-center gap-2">
              <h2 className="text-sm font-semibold tracking-tight">Bots</h2>
              <span className="rounded-full bg-muted px-2 py-0.5 text-[11px] font-medium text-muted-foreground">
                {bots.length}
              </span>
              {hasControllableManagedBots ? (
                <MembersSidebarAgentControls
                  canBulkRemove={hasRemovableManagedBots}
                  canBulkRespawn={hasControllableManagedBots}
                  canBulkStop={hasStoppableManagedBots}
                  disabled={isActionPending || isArchived}
                  onRemoveAll={() => {
                    void handleRemoveAll();
                  }}
                  onRespawnAll={() => {
                    void handleRespawnAll();
                  }}
                  onStopAll={() => {
                    void handleStopAll();
                  }}
                />
              ) : null}
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

          {actionNoticeMessage ? (
            <p
              className="text-sm text-muted-foreground"
              data-testid="members-sidebar-action-notice"
            >
              {actionNoticeMessage}
            </p>
          ) : null}

          {actionErrorMessage ? (
            <p
              className="text-sm text-destructive"
              data-testid="members-sidebar-action-error"
            >
              {actionErrorMessage}
            </p>
          ) : null}
        </div>
      </SheetContent>
    </Sheet>
  );
}
