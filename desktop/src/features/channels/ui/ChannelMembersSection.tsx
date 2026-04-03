import { Crown, Shield, User } from "lucide-react";

import {
  useAddChannelMembersMutation,
  useRemoveChannelMemberMutation,
} from "@/features/channels/hooks";
import { usePresenceQuery } from "@/features/presence/hooks";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import type { ChannelMember } from "@/shared/api/types";
import { truncatePubkey } from "@/shared/lib/pubkey";
import { Button } from "@/shared/ui/button";
import { Section } from "@/shared/ui/Section";
import { ChannelMemberInviteCard } from "./ChannelMemberInviteCard";

type ChannelMembersSectionProps = {
  channelId: string | null;
  channelType: string;
  canManageChannel: boolean;
  currentPubkey?: string;
  isArchived: boolean;
  members: ChannelMember[];
  membersLoading: boolean;
  open: boolean;
  onSelfRemoved: () => void;
};

function formatMemberName(member: ChannelMember, currentPubkey?: string) {
  if (currentPubkey && member.pubkey === currentPubkey) {
    return "You";
  }
  return member.displayName ?? truncatePubkey(member.pubkey);
}

function roleIcon(role: ChannelMember["role"]) {
  switch (role) {
    case "owner":
      return Crown;
    case "admin":
      return Shield;
    default:
      return User;
  }
}

export function ChannelMembersSection({
  channelId,
  channelType,
  canManageChannel,
  currentPubkey,
  isArchived,
  members,
  membersLoading,
  open,
  onSelfRemoved,
}: ChannelMembersSectionProps) {
  const addMembersMutation = useAddChannelMembersMutation(channelId);
  const removeMemberMutation = useRemoveChannelMemberMutation(channelId);
  const memberPresenceQuery = usePresenceQuery(
    members.map((member) => member.pubkey),
    { enabled: open && members.length > 0 },
  );

  return (
    <Section
      description="Owners and admins can invite members or remove them."
      title="Members"
    >
      {canManageChannel && channelType !== "dm" ? (
        <ChannelMemberInviteCard
          existingMembers={members}
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

      <div className="space-y-2" data-testid="channel-members-list">
        {members.length > 0 ? (
          members.map((member) => {
            const Icon = roleIcon(member.role);

            return (
              <div
                className="flex items-start justify-between gap-3 rounded-2xl border border-border/80 bg-background px-4 py-3"
                data-testid={`channel-member-${member.pubkey}`}
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
                        data-testid={`member-presence-${member.pubkey}`}
                        status={
                          memberPresenceQuery.data[
                            member.pubkey.toLowerCase()
                          ] ?? "offline"
                        }
                      />
                    ) : null}
                    <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                      {member.role}
                    </span>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {member.pubkey}
                  </p>
                </div>
                {canManageChannel ||
                (currentPubkey && member.pubkey === currentPubkey) ? (
                  <Button
                    data-testid={`remove-member-${member.pubkey}`}
                    disabled={removeMemberMutation.isPending || isArchived}
                    onClick={() => {
                      void removeMemberMutation
                        .mutateAsync(member.pubkey)
                        .then(() => {
                          if (member.pubkey === currentPubkey) {
                            onSelfRemoved();
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
          })
        ) : (
          <p className="text-sm text-muted-foreground">
            {membersLoading ? "Loading members..." : "No active members found."}
          </p>
        )}
      </div>

      {removeMemberMutation.error instanceof Error ? (
        <p className="text-sm text-destructive">
          {removeMemberMutation.error.message}
        </p>
      ) : null}
    </Section>
  );
}
