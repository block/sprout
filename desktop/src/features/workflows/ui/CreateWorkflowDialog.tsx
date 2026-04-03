import { ChevronDown } from "lucide-react";
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

  const reset = React.useCallback(() => {
    setSelectedChannelId(channels[0]?.id ?? "");
    setYamlDefinition("");
    createMutation.reset();
  }, [channels, createMutation]);

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
              <label
                className="block text-sm font-medium"
                htmlFor="wf-channel-select"
              >
                Channel
              </label>
              <div className="relative">
                <select
                  className="flex h-11 w-full appearance-none rounded-xl border border-border/70 bg-muted/20 px-3 pr-10 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={createMutation.isPending}
                  id="wf-channel-select"
                  onChange={(event) => {
                    createMutation.reset();
                    setSelectedChannelId(event.target.value);
                  }}
                  value={selectedChannelId}
                >
                  {channels.map((channel) => (
                    <option key={channel.id} value={channel.id}>
                      {channel.name} · {channel.channelType} ·{" "}
                      {channel.visibility}
                    </option>
                  ))}
                </select>
                <ChevronDown className="pointer-events-none absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
              </div>
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
