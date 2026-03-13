import {
  Archive,
  ArchiveRestore,
  Crown,
  DoorClosed,
  DoorOpen,
  FileText,
  Hash,
  Lock,
  MessageSquare,
  Shield,
  User,
  UserPlus,
  Users,
} from "lucide-react";
import * as React from "react";

import {
  useAddChannelMembersMutation,
  useArchiveChannelMutation,
  useChannelDetailsQuery,
  useChannelMembersQuery,
  useDeleteChannelMutation,
  useJoinChannelMutation,
  useLeaveChannelMutation,
  useRemoveChannelMemberMutation,
  useSetChannelPurposeMutation,
  useSetChannelTopicMutation,
  useUnarchiveChannelMutation,
  useUpdateChannelMutation,
} from "@/features/channels/hooks";
import { usePresenceQuery } from "@/features/presence/hooks";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import type { Channel, ChannelMember } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Separator } from "@/shared/ui/separator";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/shared/ui/sheet";
import { Textarea } from "@/shared/ui/textarea";

type ChannelManagementSheetProps = {
  channel: Channel | null;
  currentPubkey?: string;
  onDeleted?: () => void;
  onOpenChange: (open: boolean) => void;
  open: boolean;
};

const roleOptions: Array<Exclude<ChannelMember["role"], "owner">> = [
  "member",
  "admin",
  "guest",
  "bot",
];

const roleOrder: Record<ChannelMember["role"], number> = {
  owner: 0,
  admin: 1,
  member: 2,
  guest: 3,
  bot: 4,
};

function formatPubkey(pubkey: string) {
  return `${pubkey.slice(0, 8)}…${pubkey.slice(-4)}`;
}

function formatMemberName(member: ChannelMember, currentPubkey?: string) {
  if (currentPubkey && member.pubkey === currentPubkey) {
    return "You";
  }

  return member.displayName ?? formatPubkey(member.pubkey);
}

function Section({
  title,
  description,
  children,
}: React.PropsWithChildren<{
  title: string;
  description?: string;
}>) {
  return (
    <section className="space-y-3">
      <div className="space-y-1">
        <h2 className="text-sm font-semibold tracking-tight">{title}</h2>
        {description ? (
          <p className="text-sm text-muted-foreground">{description}</p>
        ) : null}
      </div>
      {children}
    </section>
  );
}

function MetadataPill({
  icon: Icon,
  label,
}: {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
}) {
  return (
    <div className="inline-flex items-center gap-2 rounded-full border border-border/80 bg-muted/40 px-3 py-1 text-xs font-medium text-muted-foreground">
      <Icon className="h-3.5 w-3.5" />
      <span>{label}</span>
    </div>
  );
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

export function ChannelManagementSheet({
  channel,
  currentPubkey,
  onDeleted,
  onOpenChange,
  open,
}: ChannelManagementSheetProps) {
  const channelId = channel?.id ?? null;
  const detailsQuery = useChannelDetailsQuery(channelId, open);
  const membersQuery = useChannelMembersQuery(channelId, open);
  const updateChannelMutation = useUpdateChannelMutation(channelId);
  const setTopicMutation = useSetChannelTopicMutation(channelId);
  const setPurposeMutation = useSetChannelPurposeMutation(channelId);
  const archiveChannelMutation = useArchiveChannelMutation(channelId);
  const unarchiveChannelMutation = useUnarchiveChannelMutation(channelId);
  const deleteChannelMutation = useDeleteChannelMutation(channelId);
  const addMembersMutation = useAddChannelMembersMutation(channelId);
  const removeMemberMutation = useRemoveChannelMemberMutation(channelId);
  const joinChannelMutation = useJoinChannelMutation(channelId);
  const leaveChannelMutation = useLeaveChannelMutation(channelId);

  const detail = detailsQuery.data ?? channel;
  const members = React.useMemo(() => {
    const currentMembers = membersQuery.data ?? [];
    return [...currentMembers].sort((left, right) => {
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
  }, [currentPubkey, membersQuery.data]);
  const memberPresenceQuery = usePresenceQuery(
    members.map((member) => member.pubkey),
    { enabled: open && members.length > 0 },
  );

  const selfMember =
    members.find((member) => member.pubkey === currentPubkey) ?? null;
  const hasResolvedMembership = membersQuery.data !== undefined;
  const isOwner = selfMember?.role === "owner";
  const canManageChannel =
    selfMember?.role === "owner" || selfMember?.role === "admin";
  const canEditNarrative = selfMember !== null && detail?.channelType !== "dm";
  const isArchived =
    detail?.archivedAt !== null && detail?.archivedAt !== undefined;
  const canJoin =
    hasResolvedMembership &&
    detail?.channelType !== "dm" &&
    detail?.visibility === "open" &&
    !isArchived &&
    selfMember === null;
  const canLeave =
    hasResolvedMembership &&
    detail?.channelType !== "dm" &&
    !isArchived &&
    selfMember !== null;

  const [nameDraft, setNameDraft] = React.useState("");
  const [descriptionDraft, setDescriptionDraft] = React.useState("");
  const [topicDraft, setTopicDraft] = React.useState("");
  const [purposeDraft, setPurposeDraft] = React.useState("");
  const [invitePubkeys, setInvitePubkeys] = React.useState("");
  const [inviteRole, setInviteRole] =
    React.useState<Exclude<ChannelMember["role"], "owner">>("member");

  // Sync drafts from server only when the sheet opens or the channel changes —
  // not on every background refetch, which would clobber in-flight edits.
  const syncedForRef = React.useRef<string | null>(null);
  React.useEffect(() => {
    if (!detail) {
      return;
    }

    const key = `${detail.id}:${open}`;
    if (!open || syncedForRef.current === key) {
      return;
    }
    syncedForRef.current = key;

    setNameDraft(detail.name);
    setDescriptionDraft(detail.description);
    setTopicDraft(detail.topic ?? "");
    setPurposeDraft(detail.purpose ?? "");
  }, [detail, open]);

  if (!channel) {
    return null;
  }

  const resolvedChannel = detail ?? channel;

  const parsedInvitePubkeys = invitePubkeys
    .split(/[\s,]+/)
    .map((value) => value.trim())
    .filter((value) => value.length > 0);

  return (
    <Sheet onOpenChange={onOpenChange} open={open}>
      <SheetContent
        className="flex w-full flex-col gap-0 overflow-hidden border-l border-border/80 bg-background p-0 sm:max-w-xl"
        data-testid="channel-management-sheet"
        side="right"
      >
        <SheetHeader className="space-y-4 border-b border-border/80 bg-muted/20 px-6 py-6 text-left">
          <div className="space-y-2">
            <SheetTitle className="pr-8">{channel.name}</SheetTitle>
            <SheetDescription>
              Manage channel settings, membership, and access.
            </SheetDescription>
          </div>
          <div className="flex flex-wrap gap-2">
            <MetadataPill
              icon={
                channel.channelType === "forum"
                  ? FileText
                  : channel.channelType === "dm"
                    ? MessageSquare
                    : Hash
              }
              label={channel.channelType}
            />
            <MetadataPill
              icon={channel.visibility === "private" ? Lock : DoorOpen}
              label={channel.visibility}
            />
            <MetadataPill
              icon={Users}
              label={`${resolvedChannel.memberCount} members`}
            />
            {isArchived ? (
              <MetadataPill icon={Archive} label="archived" />
            ) : null}
          </div>
        </SheetHeader>

        <div className="flex-1 space-y-6 overflow-y-auto px-6 py-6">
          {detailsQuery.error instanceof Error ? (
            <p className="rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {detailsQuery.error.message}
            </p>
          ) : null}

          {membersQuery.error instanceof Error ? (
            <p className="rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {membersQuery.error.message}
            </p>
          ) : null}

          <Section
            description="Open channels stay visible to everyone. Private channels require an invite."
            title="Access"
          >
            <div className="flex flex-wrap gap-2">
              {canJoin ? (
                <Button
                  data-testid="channel-management-join"
                  disabled={joinChannelMutation.isPending}
                  onClick={() => {
                    void joinChannelMutation.mutateAsync();
                  }}
                  size="sm"
                  type="button"
                >
                  <DoorOpen className="h-4 w-4" />
                  {joinChannelMutation.isPending
                    ? "Joining..."
                    : "Join channel"}
                </Button>
              ) : null}

              {canLeave ? (
                <Button
                  data-testid="channel-management-leave"
                  disabled={leaveChannelMutation.isPending}
                  onClick={() => {
                    void leaveChannelMutation.mutateAsync().then(() => {
                      onOpenChange(false);
                    });
                  }}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  <DoorClosed className="h-4 w-4" />
                  {leaveChannelMutation.isPending
                    ? "Leaving..."
                    : "Leave channel"}
                </Button>
              ) : null}
            </div>
            {joinChannelMutation.error instanceof Error ? (
              <p className="text-sm text-destructive">
                {joinChannelMutation.error.message}
              </p>
            ) : null}
            {leaveChannelMutation.error instanceof Error ? (
              <p className="text-sm text-destructive">
                {leaveChannelMutation.error.message}
              </p>
            ) : null}
          </Section>

          <Separator />

          <Section
            description="Name and description are owner/admin actions."
            title="Details"
          >
            <form
              className="space-y-3"
              onSubmit={(event) => {
                event.preventDefault();
                void updateChannelMutation.mutateAsync({
                  description: descriptionDraft.trim() || undefined,
                  name: nameDraft.trim() || undefined,
                });
              }}
            >
              <div className="space-y-1.5">
                <label className="text-sm font-medium" htmlFor="channel-name">
                  Name
                </label>
                <Input
                  data-testid="channel-management-name"
                  disabled={
                    !canManageChannel || updateChannelMutation.isPending
                  }
                  id="channel-name"
                  onChange={(event) => setNameDraft(event.target.value)}
                  value={nameDraft}
                />
              </div>
              <div className="space-y-1.5">
                <label
                  className="text-sm font-medium"
                  htmlFor="channel-description"
                >
                  Description
                </label>
                <Textarea
                  className="min-h-24"
                  data-testid="channel-management-description"
                  disabled={
                    !canManageChannel || updateChannelMutation.isPending
                  }
                  id="channel-description"
                  onChange={(event) => setDescriptionDraft(event.target.value)}
                  value={descriptionDraft}
                />
              </div>
              <Button
                data-testid="channel-management-save-details"
                disabled={!canManageChannel || updateChannelMutation.isPending}
                size="sm"
                type="submit"
              >
                {updateChannelMutation.isPending ? "Saving..." : "Save details"}
              </Button>
              {updateChannelMutation.error instanceof Error ? (
                <p className="text-sm text-destructive">
                  {updateChannelMutation.error.message}
                </p>
              ) : null}
            </form>
          </Section>

          <Separator />

          <Section
            description="Topic and purpose show the current context for the channel."
            title="Context"
          >
            <form
              className="space-y-3"
              onSubmit={(event) => {
                event.preventDefault();
                void setTopicMutation.mutateAsync({
                  topic: topicDraft.trim(),
                });
              }}
            >
              <div className="space-y-1.5">
                <label className="text-sm font-medium" htmlFor="channel-topic">
                  Topic
                </label>
                <Input
                  data-testid="channel-management-topic"
                  disabled={!canEditNarrative || setTopicMutation.isPending}
                  id="channel-topic"
                  onChange={(event) => setTopicDraft(event.target.value)}
                  value={topicDraft}
                />
              </div>
              <Button
                data-testid="channel-management-save-topic"
                disabled={!canEditNarrative || setTopicMutation.isPending}
                size="sm"
                type="submit"
                variant="outline"
              >
                {setTopicMutation.isPending ? "Saving..." : "Save topic"}
              </Button>
              {setTopicMutation.error instanceof Error ? (
                <p className="text-sm text-destructive">
                  {setTopicMutation.error.message}
                </p>
              ) : null}
            </form>

            <form
              className="space-y-3"
              onSubmit={(event) => {
                event.preventDefault();
                void setPurposeMutation.mutateAsync({
                  purpose: purposeDraft.trim(),
                });
              }}
            >
              <div className="space-y-1.5">
                <label
                  className="text-sm font-medium"
                  htmlFor="channel-purpose"
                >
                  Purpose
                </label>
                <Textarea
                  className="min-h-24"
                  data-testid="channel-management-purpose"
                  disabled={!canEditNarrative || setPurposeMutation.isPending}
                  id="channel-purpose"
                  onChange={(event) => setPurposeDraft(event.target.value)}
                  value={purposeDraft}
                />
              </div>
              <Button
                data-testid="channel-management-save-purpose"
                disabled={!canEditNarrative || setPurposeMutation.isPending}
                size="sm"
                type="submit"
                variant="outline"
              >
                {setPurposeMutation.isPending ? "Saving..." : "Save purpose"}
              </Button>
              {setPurposeMutation.error instanceof Error ? (
                <p className="text-sm text-destructive">
                  {setPurposeMutation.error.message}
                </p>
              ) : null}
            </form>
          </Section>

          <Separator />

          <Section
            description="Owners and admins can invite members or remove them."
            title="Members"
          >
            {canManageChannel && resolvedChannel.channelType !== "dm" ? (
              <form
                className="space-y-3 rounded-2xl border border-border/80 bg-muted/20 p-4"
                onSubmit={(event) => {
                  event.preventDefault();
                  void addMembersMutation
                    .mutateAsync({
                      pubkeys: parsedInvitePubkeys,
                      role: inviteRole,
                    })
                    .then((result) => {
                      if (result.errors.length === 0) {
                        setInvitePubkeys("");
                      }
                    });
                }}
              >
                <div className="flex items-center gap-2 text-sm font-medium">
                  <UserPlus className="h-4 w-4" />
                  Invite members
                </div>
                <Textarea
                  className="min-h-24"
                  data-testid="channel-management-add-pubkeys"
                  disabled={addMembersMutation.isPending}
                  onChange={(event) => setInvitePubkeys(event.target.value)}
                  placeholder="Paste one or more pubkeys, separated by spaces, commas, or new lines."
                  value={invitePubkeys}
                />
                <div className="flex flex-wrap items-center gap-3">
                  <label
                    className="flex items-center gap-2 text-sm text-muted-foreground"
                    htmlFor="channel-member-role"
                  >
                    Role
                  </label>
                  <select
                    className="h-9 rounded-md border border-input bg-background px-3 text-sm"
                    data-testid="channel-management-add-role"
                    disabled={addMembersMutation.isPending}
                    id="channel-member-role"
                    onChange={(event) =>
                      setInviteRole(
                        event.target.value as Exclude<
                          ChannelMember["role"],
                          "owner"
                        >,
                      )
                    }
                    value={inviteRole}
                  >
                    {roleOptions.map((role) => (
                      <option key={role} value={role}>
                        {role}
                      </option>
                    ))}
                  </select>
                  <Button
                    data-testid="channel-management-add-members"
                    disabled={
                      addMembersMutation.isPending ||
                      parsedInvitePubkeys.length === 0
                    }
                    size="sm"
                    type="submit"
                  >
                    {addMembersMutation.isPending
                      ? "Inviting..."
                      : "Add members"}
                  </Button>
                </div>
                {addMembersMutation.error instanceof Error ? (
                  <p className="text-sm text-destructive">
                    {addMembersMutation.error.message}
                  </p>
                ) : null}
                {addMembersMutation.data &&
                addMembersMutation.data.errors.length > 0 ? (
                  <div className="space-y-1 text-sm text-destructive">
                    {addMembersMutation.data.errors.map((error) => (
                      <p key={`${error.pubkey}-${error.error}`}>
                        {formatPubkey(error.pubkey)}: {error.error}
                      </p>
                    ))}
                  </div>
                ) : null}
              </form>
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
                          disabled={
                            removeMemberMutation.isPending || isArchived
                          }
                          onClick={() => {
                            void removeMemberMutation
                              .mutateAsync(member.pubkey)
                              .then(() => {
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
                })
              ) : (
                <p className="text-sm text-muted-foreground">
                  {membersQuery.isLoading
                    ? "Loading members..."
                    : "No active members found."}
                </p>
              )}
            </div>

            {removeMemberMutation.error instanceof Error ? (
              <p className="text-sm text-destructive">
                {removeMemberMutation.error.message}
              </p>
            ) : null}
          </Section>

          {resolvedChannel.channelType !== "dm" ? (
            <>
              <Separator />

              <Section
                description="Archiving keeps history but blocks new changes."
                title="Channel state"
              >
                <div className="flex flex-wrap gap-2">
                  {isArchived ? (
                    <Button
                      data-testid="channel-management-unarchive"
                      disabled={
                        !canManageChannel || unarchiveChannelMutation.isPending
                      }
                      onClick={() => {
                        void unarchiveChannelMutation.mutateAsync();
                      }}
                      size="sm"
                      type="button"
                    >
                      <ArchiveRestore className="h-4 w-4" />
                      {unarchiveChannelMutation.isPending
                        ? "Restoring..."
                        : "Unarchive channel"}
                    </Button>
                  ) : (
                    <Button
                      data-testid="channel-management-archive"
                      disabled={
                        !canManageChannel || archiveChannelMutation.isPending
                      }
                      onClick={() => {
                        void archiveChannelMutation.mutateAsync();
                      }}
                      size="sm"
                      type="button"
                      variant="outline"
                    >
                      <Archive className="h-4 w-4" />
                      {archiveChannelMutation.isPending
                        ? "Archiving..."
                        : "Archive channel"}
                    </Button>
                  )}
                </div>
                {archiveChannelMutation.error instanceof Error ? (
                  <p className="text-sm text-destructive">
                    {archiveChannelMutation.error.message}
                  </p>
                ) : null}
                {unarchiveChannelMutation.error instanceof Error ? (
                  <p className="text-sm text-destructive">
                    {unarchiveChannelMutation.error.message}
                  </p>
                ) : null}
              </Section>
            </>
          ) : null}

          {isOwner && resolvedChannel.channelType !== "dm" ? (
            <>
              <Separator />

              <Section
                description="Deleting removes the channel from the workspace list."
                title="Danger zone"
              >
                <Button
                  data-testid="channel-management-delete"
                  disabled={deleteChannelMutation.isPending}
                  onClick={() => {
                    if (!window.confirm(`Delete ${resolvedChannel.name}?`)) {
                      return;
                    }

                    void deleteChannelMutation.mutateAsync().then(() => {
                      onOpenChange(false);
                      onDeleted?.();
                    });
                  }}
                  size="sm"
                  type="button"
                  variant="destructive"
                >
                  {deleteChannelMutation.isPending
                    ? "Deleting..."
                    : "Delete channel"}
                </Button>
                {deleteChannelMutation.error instanceof Error ? (
                  <p className="text-sm text-destructive">
                    {deleteChannelMutation.error.message}
                  </p>
                ) : null}
              </Section>
            </>
          ) : null}
        </div>
      </SheetContent>
    </Sheet>
  );
}
