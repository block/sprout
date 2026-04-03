import {
  Archive,
  DoorClosed,
  DoorOpen,
  FileText,
  Hash,
  Lock,
  MessageSquare,
  Users,
} from "lucide-react";
import * as React from "react";

import {
  useChannelDetailsQuery,
  useChannelMembersQuery,
  useJoinChannelMutation,
  useLeaveChannelMutation,
} from "@/features/channels/hooks";
import { truncatePubkey } from "@/shared/lib/pubkey";
import type { Channel, ChannelMember } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { Section } from "@/shared/ui/Section";
import { Separator } from "@/shared/ui/separator";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/shared/ui/sheet";
import { ChannelDangerZone } from "./ChannelDangerZone";
import { ChannelDetailsSection } from "./ChannelDetailsSection";
import { ChannelMembersSection } from "./ChannelMembersSection";

type ChannelManagementSheetProps = {
  channel: Channel | null;
  currentPubkey?: string;
  onDeleted?: () => void;
  onOpenChange: (open: boolean) => void;
  open: boolean;
};

const roleOrder: Record<ChannelMember["role"], number> = {
  owner: 0,
  admin: 1,
  member: 2,
  guest: 3,
  bot: 4,
};

function formatMemberName(member: ChannelMember, currentPubkey?: string) {
  if (currentPubkey && member.pubkey === currentPubkey) {
    return "You";
  }
  return member.displayName ?? truncatePubkey(member.pubkey);
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
  const joinChannelMutation = useJoinChannelMutation(channelId);
  const leaveChannelMutation = useLeaveChannelMutation(channelId);

  const detail = detailsQuery.data ?? channel;
  const members = React.useMemo(() => {
    const currentMembers = membersQuery.data ?? [];
    return [...currentMembers].sort((left, right) => {
      if (currentPubkey && left.pubkey === currentPubkey) return -1;
      if (currentPubkey && right.pubkey === currentPubkey) return 1;
      const roleDelta = roleOrder[left.role] - roleOrder[right.role];
      if (roleDelta !== 0) return roleDelta;
      return formatMemberName(left).localeCompare(formatMemberName(right));
    });
  }, [currentPubkey, membersQuery.data]);

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

  // Sync drafts from server only when the sheet opens or the channel changes —
  // not on every background refetch, which would clobber in-flight edits.
  const syncedForRef = React.useRef<string | null>(null);
  React.useEffect(() => {
    if (!open) {
      syncedForRef.current = null;
      return;
    }
    if (!detail) return;

    const key = detail.id;
    if (syncedForRef.current === key) return;
    syncedForRef.current = key;

    setNameDraft(detail.name);
    setDescriptionDraft(detail.description);
    setTopicDraft(detail.topic ?? "");
    setPurposeDraft(detail.purpose ?? "");
  }, [detail, open]);

  if (!channel) return null;

  const resolvedChannel = detail ?? channel;

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

          <ChannelDetailsSection
            canEditNarrative={canEditNarrative}
            canManageChannel={canManageChannel}
            channelId={channelId}
            descriptionDraft={descriptionDraft}
            isArchived={isArchived}
            nameDraft={nameDraft}
            onDescriptionChange={setDescriptionDraft}
            onNameChange={setNameDraft}
            onPurposeChange={setPurposeDraft}
            onTopicChange={setTopicDraft}
            purposeDraft={purposeDraft}
            topicDraft={topicDraft}
          />

          <Separator />

          <ChannelMembersSection
            canManageChannel={canManageChannel}
            channelId={channelId}
            channelType={resolvedChannel.channelType}
            currentPubkey={currentPubkey}
            isArchived={isArchived}
            members={members}
            membersLoading={membersQuery.isLoading}
            onSelfRemoved={() => onOpenChange(false)}
            open={open}
          />

          {resolvedChannel.channelType !== "dm" ? (
            <ChannelDangerZone
              canManageChannel={canManageChannel}
              channelId={channelId}
              channelName={resolvedChannel.name}
              isArchived={isArchived}
              isOwner={isOwner}
              onDeleted={() => {
                onOpenChange(false);
                onDeleted?.();
              }}
            />
          ) : null}
        </div>
      </SheetContent>
    </Sheet>
  );
}
