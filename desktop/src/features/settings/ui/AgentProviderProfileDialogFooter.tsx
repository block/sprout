/**
 * Footer row for AgentProviderProfileDialog: Cancel + Save, plus a
 * two-click Delete on the left when editing an existing profile.
 *
 * The Delete arm uses the same two-click confirmation pattern as the
 * row-level delete in the settings card so users learn one gesture.
 */
import { toast } from "sonner";

import { useDeleteAgentProviderProfileMutation } from "@/features/settings/hooks/useAgentProviderSettings.ts";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";

export function AgentProviderProfileDialogFooter({
  deleteArmed,
  isEdit,
  isSaving,
  onCancel,
  onDeleted,
  onSetDeleteArmed,
  profileId,
  saveDisabled,
}: {
  deleteArmed: boolean;
  isEdit: boolean;
  isSaving: boolean;
  onCancel: () => void;
  onDeleted: () => void;
  onSetDeleteArmed: (armed: boolean) => void;
  profileId: string | null;
  saveDisabled: boolean;
}) {
  const deleteMutation = useDeleteAgentProviderProfileMutation();

  return (
    <div className="flex items-center gap-2 pt-2">
      {isEdit && profileId !== null ? (
        <Button
          className={cn(deleteArmed && "text-red-600 dark:text-red-400")}
          data-testid="agent-provider-profile-delete"
          disabled={deleteMutation.isPending}
          onClick={async () => {
            if (!deleteArmed) {
              onSetDeleteArmed(true);
              return;
            }
            try {
              await deleteMutation.mutateAsync(profileId);
              toast.success("Profile deleted");
              onDeleted();
            } catch (err) {
              toast.error(err instanceof Error ? err.message : String(err));
              onSetDeleteArmed(false);
            }
          }}
          type="button"
          variant="ghost"
        >
          {deleteMutation.isPending
            ? "Deleting…"
            : deleteArmed
              ? "Click to confirm"
              : "Delete"}
        </Button>
      ) : null}
      <div className="ml-auto flex items-center gap-2">
        <Button
          data-testid="agent-provider-profile-cancel"
          onClick={onCancel}
          type="button"
          variant="ghost"
        >
          Cancel
        </Button>
        <Button
          data-testid="agent-provider-profile-save"
          disabled={saveDisabled}
          type="submit"
        >
          {isSaving ? "Saving…" : isEdit ? "Save" : "Add profile"}
        </Button>
      </div>
    </div>
  );
}
