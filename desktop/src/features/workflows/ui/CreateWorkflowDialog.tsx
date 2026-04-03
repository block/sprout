import * as React from "react";

import { useCreateWorkflowMutation } from "@/features/workflows/hooks";
import type { Channel } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Textarea } from "@/shared/ui/textarea";

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

  React.useEffect(() => {
    if (open && channels.length > 0 && !selectedChannelId) {
      setSelectedChannelId(channels[0].id);
    }
  }, [open, channels, selectedChannelId]);

  function handleCreate() {
    if (!selectedChannelId || !yamlDefinition.trim()) return;

    createMutation.mutate(yamlDefinition, {
      onSuccess: () => {
        setYamlDefinition("");
        onOpenChange(false);
      },
    });
  }

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Create Workflow</DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          <div>
            <label
              className="mb-1 block text-sm font-medium"
              htmlFor="wf-channel-select"
            >
              Channel
            </label>
            <select
              className="w-full rounded-md border bg-background px-3 py-2 text-sm"
              id="wf-channel-select"
              onChange={(event) => setSelectedChannelId(event.target.value)}
              value={selectedChannelId}
            >
              {channels.map((channel) => (
                <option key={channel.id} value={channel.id}>
                  {channel.name}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label
              className="mb-1 block text-sm font-medium"
              htmlFor="wf-yaml-editor"
            >
              YAML Definition
            </label>
            <Textarea
              className="min-h-[200px] resize-y font-mono text-xs"
              id="wf-yaml-editor"
              onChange={(event) => setYamlDefinition(event.target.value)}
              placeholder="name: my-workflow&#10;trigger:&#10;  type: manual&#10;steps:&#10;  - id: step-1&#10;    action: ..."
              value={yamlDefinition}
            />
          </div>
        </div>

        <DialogFooter>
          <Button
            onClick={() => onOpenChange(false)}
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
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
