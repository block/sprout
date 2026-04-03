import { Archive, ArchiveRestore } from "lucide-react";
import * as React from "react";

import {
  useArchiveChannelMutation,
  useDeleteChannelMutation,
  useUnarchiveChannelMutation,
} from "@/features/channels/hooks";
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
import { Section } from "@/shared/ui/Section";
import { Separator } from "@/shared/ui/separator";

type ChannelDangerZoneProps = {
  channelId: string | null;
  channelName: string;
  canManageChannel: boolean;
  isOwner: boolean;
  isArchived: boolean;
  onDeleted: () => void;
};

export function ChannelDangerZone({
  channelId,
  channelName,
  canManageChannel,
  isOwner,
  isArchived,
  onDeleted,
}: ChannelDangerZoneProps) {
  const archiveChannelMutation = useArchiveChannelMutation(channelId);
  const unarchiveChannelMutation = useUnarchiveChannelMutation(channelId);
  const deleteChannelMutation = useDeleteChannelMutation(channelId);
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = React.useState(false);

  function handleDeleteDialogOpenChange(next: boolean) {
    deleteChannelMutation.reset();
    setIsDeleteDialogOpen(next);
  }

  async function handleDeleteChannel() {
    try {
      await deleteChannelMutation.mutateAsync();
      handleDeleteDialogOpenChange(false);
      onDeleted();
    } catch {
      // Error rendered inline in the confirmation dialog.
    }
  }

  return (
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
              disabled={!canManageChannel || unarchiveChannelMutation.isPending}
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
              disabled={!canManageChannel || archiveChannelMutation.isPending}
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

      {isOwner ? (
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
                    Delete {channelName} from the workspace list. This action
                    cannot be undone.
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
    </>
  );
}
