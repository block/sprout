import {
  Archive,
  ArchiveRestore,
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
  useArchiveChannelMutation,
  useChannelDetailsQuery,
  useChannelMembersQuery,
  useDeleteChannelMutation,
  useJoinChannelMutation,
  useLeaveChannelMutation,
  useSetChannelPurposeMutation,
  useSetChannelTopicMutation,
  useUnarchiveChannelMutation,
  useUpdateChannelMutation,
} from "@/features/channels/hooks";
import { compareMembersByRole } from "@/features/channels/lib/memberUtils";
import type { Channel } from "@/shared/api/types";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/shared/ui/alert-dialog";
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
import { ChannelCanvas } from "./ChannelCanvas";

type ChannelManagementSheetProps = {
  channel: Channel | null;
  currentPubkey?: string;
  onDeleted?: () => void;
  onOpenChange: (open: boolean) => void;
  open: boolean;
};

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
  const joinChannelMutation = useJoinChannelMutation(channelId);
  const leaveChannelMutation = useLeaveChannelMutation(channelId);

  const detail = detailsQuery.data ?? channel;
  const members = React.useMemo(() => {
    const currentMembers = membersQuery.data ?? [];
    return [...currentMembers].sort((left, right) =>
      compareMembersByRole(left, right, currentPubkey),
    );
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
  const showAccessSection =
    canJoin ||
    canLeave ||
    joinChannelMutation.error instanceof Error ||
    leaveChannelMutation.error instanceof Error;

  const [nameDraft, setNameDraft] = React.useState("");
  const [descriptionDraft, setDescriptionDraft] = React.useState("");
  const [topicDraft, setTopicDraft] = React.useState("");
  const [purposeDraft, setPurposeDraft] = React.useState("");
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = React.useState(false);

  // Sync drafts from server only when the sheet opens or the channel changes —
  // not on every background refetch, which would clobber in-flight edits.
  const syncedForRef = React.useRef<string | null>(null);
  React.useEffect(() => {
    if (!open) {
      // Reset on close so the next open re-syncs from server.
      syncedForRef.current = null;
      setIsDeleteDialogOpen(false);
      return;
    }
    if (!detail) {
      return;
    }

    const key = detail.id;
    if (syncedForRef.current === key) {
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

  function handleDeleteDialogOpenChange(next: boolean) {
    deleteChannelMutation.reset();
    setIsDeleteDialogOpen(next);
  }

  async function handleDeleteChannel() {
    try {
      await deleteChannelMutation.mutateAsync();
      handleDeleteDialogOpenChange(false);
      onOpenChange(false);
      onDeleted?.();
    } catch {
      // The mutation error is rendered inline in the confirmation dialog.
    }
  }

  function handleSheetOpenChange(next: boolean) {
    if (!next) {
      handleDeleteDialogOpenChange(false);
    }

    onOpenChange(next);
  }

  const resolvedChannel = detail ?? channel;

  return (
    <Sheet onOpenChange={handleSheetOpenChange} open={open}>
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

          {showAccessSection ? (
            <>
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
            </>
          ) : null}

          <Section
            description="A shared Markdown document for the channel."
            title="Canvas"
          >
            <ChannelCanvas
              canEdit={canEditNarrative}
              channelId={channelId}
              isArchived={isArchived}
            />
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
                <AlertDialog
                  onOpenChange={handleDeleteDialogOpenChange}
                  open={isDeleteDialogOpen}
                >
                  <AlertDialogTrigger asChild>
                    <Button
                      data-testid="channel-management-delete"
                      disabled={deleteChannelMutation.isPending}
                      size="sm"
                      type="button"
                      variant="destructive"
                    >
                      Delete channel
                    </Button>
                  </AlertDialogTrigger>
                  <AlertDialogContent data-testid="channel-delete-confirmation-dialog">
                    <AlertDialogHeader>
                      <AlertDialogTitle>Delete channel?</AlertDialogTitle>
                      <AlertDialogDescription>
                        Delete {resolvedChannel.name} from the workspace list.
                        This action cannot be undone.
                      </AlertDialogDescription>
                    </AlertDialogHeader>
                    {deleteChannelMutation.error instanceof Error ? (
                      <p className="text-sm text-destructive">
                        {deleteChannelMutation.error.message}
                      </p>
                    ) : null}
                    <AlertDialogFooter>
                      <AlertDialogCancel asChild>
                        <Button
                          data-testid="channel-delete-cancel"
                          disabled={deleteChannelMutation.isPending}
                          type="button"
                          variant="outline"
                        >
                          Cancel
                        </Button>
                      </AlertDialogCancel>
                      <AlertDialogAction asChild>
                        <Button
                          data-testid="channel-delete-confirm"
                          disabled={deleteChannelMutation.isPending}
                          onClick={(event) => {
                            event.preventDefault();
                            void handleDeleteChannel();
                          }}
                          type="button"
                          variant="destructive"
                        >
                          {deleteChannelMutation.isPending
                            ? "Deleting..."
                            : "Delete channel"}
                        </Button>
                      </AlertDialogAction>
                    </AlertDialogFooter>
                  </AlertDialogContent>
                </AlertDialog>
              </Section>
            </>
          ) : null}
        </div>
      </SheetContent>
    </Sheet>
  );
}
