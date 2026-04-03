import * as React from "react";

import { useCreateWorkflowMutation } from "@/features/workflows/hooks";
import type { Channel } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { FieldLabel, FormSelect } from "./workflowFormPrimitives";
import { WorkflowFormBuilder } from "./WorkflowFormBuilder";

type CreateWorkflowDialogProps = {
  channels: Channel[];
  onOpenChange: (open: boolean) => void;
  open: boolean;
};

export function CreateWorkflowDialog({
  channels,
  onOpenChange,
  open,
}: CreateWorkflowDialogProps) {
  const [selectedChannelId, setSelectedChannelId] = React.useState(
    channels[0]?.id ?? "",
  );
  const [yamlDefinition, setYamlDefinition] = React.useState("");
  const createMutation = useCreateWorkflowMutation(selectedChannelId);
  const selectedChannel =
    channels.find((channel) => channel.id === selectedChannelId) ?? null;

  const resetMutation = createMutation.reset;
  const reset = React.useCallback(() => {
    setSelectedChannelId(channels[0]?.id ?? "");
    setYamlDefinition("");
    resetMutation();
  }, [channels, resetMutation]);

  const handleOpenChange = React.useCallback(
    (nextOpen: boolean) => {
      if (!nextOpen) {
        reset();
      }

      onOpenChange(nextOpen);
    },
    [onOpenChange, reset],
  );

  React.useEffect(() => {
    if (open && channels.length > 0 && !selectedChannelId) {
      setSelectedChannelId(channels[0].id);
    }
  }, [open, channels, selectedChannelId]);

  async function handleCreate() {
    if (!selectedChannelId || !yamlDefinition.trim()) return;

    try {
      await createMutation.mutateAsync(yamlDefinition);
      handleOpenChange(false);
    } catch {
      // React Query stores the error; keep the dialog open and render it inline.
    }
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Create Workflow</DialogTitle>
          <DialogDescription>
            {channels.length === 1
              ? "Create a workflow scoped to this channel."
              : "Define a workflow and assign it to a channel."}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {channels.length > 1 ? (
            <div className="space-y-1.5">
              <FieldLabel htmlFor="wf-channel-select">Channel</FieldLabel>
              <FormSelect
                disabled={createMutation.isPending}
                id="wf-channel-select"
                onChange={(value) => {
                  createMutation.reset();
                  setSelectedChannelId(value);
                }}
                value={selectedChannelId}
              >
                {channels.map((channel) => (
                  <option key={channel.id} value={channel.id}>
                    {channel.name} · {channel.channelType} ·{" "}
                    {channel.visibility}
                  </option>
                ))}
              </FormSelect>
              <p className="text-xs text-muted-foreground">
                {selectedChannel
                  ? `New workflows will belong to ${selectedChannel.name}.`
                  : "Join or create a channel before adding a workflow."}
              </p>
            </div>
          ) : selectedChannel ? (
            <p className="text-sm text-muted-foreground">
              This workflow will be created in{" "}
              <span className="font-medium text-foreground">
                {selectedChannel.name}
              </span>
              .
            </p>
          ) : null}

          <WorkflowFormBuilder
            disabled={createMutation.isPending}
            onChange={(yaml) => {
              createMutation.reset();
              setYamlDefinition(yaml);
            }}
            yaml={yamlDefinition}
          />
        </div>

        {createMutation.error instanceof Error ? (
          <p className="rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {createMutation.error.message}
          </p>
        ) : null}

        <div className="flex justify-end gap-2 pt-4">
          <Button
            onClick={() => handleOpenChange(false)}
            type="button"
            variant="outline"
          >
            Cancel
          </Button>
          <Button
            disabled={
              !selectedChannelId ||
              !yamlDefinition.trim() ||
              createMutation.isPending
            }
            onClick={handleCreate}
            type="button"
          >
            {createMutation.isPending ? "Creating..." : "Create"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
