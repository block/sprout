import * as React from "react";

import {
  type AttachManagedAgentToChannelResult,
  useAttachManagedAgentToChannelMutation,
} from "@/features/agents/hooks";
import { useChannelsQuery } from "@/features/channels/hooks";
import type { Channel, ChannelRole, ManagedAgent } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { CopyButton } from "./CopyButton";

export function AddAgentToChannelDialog({
  agent,
  open,
  onAdded,
  onOpenChange,
}: {
  agent: ManagedAgent | null;
  open: boolean;
  onAdded: (
    channel: Channel,
    result: AttachManagedAgentToChannelResult,
  ) => void;
  onOpenChange: (open: boolean) => void;
}) {
  const channelsQuery = useChannelsQuery();
  const [channelId, setChannelId] = React.useState("");
  const [role, setRole] = React.useState<Exclude<ChannelRole, "owner">>("bot");
  const attachAgentMutation = useAttachManagedAgentToChannelMutation(
    channelId || null,
  );
  const channels = React.useMemo(
    () =>
      (channelsQuery.data ?? []).filter(
        (channel) => channel.channelType !== "dm" && !channel.archivedAt,
      ),
    [channelsQuery.data],
  );

  function reset() {
    setChannelId("");
    setRole("bot");
    attachAgentMutation.reset();
  }

  function handleOpenChange(next: boolean) {
    if (!next) {
      reset();
    }

    onOpenChange(next);
  }

  React.useEffect(() => {
    if (!open) {
      return;
    }

    if (!channelId && channels.length > 0) {
      setChannelId(channels[0].id);
    }
  }, [channelId, channels, open]);

  const selectedChannel =
    channels.find((channel) => channel.id === channelId) ?? null;

  async function handleSubmit() {
    if (!agent || !selectedChannel) {
      return;
    }

    try {
      const result = await attachAgentMutation.mutateAsync({
        agent,
        role,
      });

      onAdded(selectedChannel, result);
      handleOpenChange(false);
    } catch {
      // React Query stores the error; keep the dialog open and render it inline.
    }
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="max-w-xl overflow-hidden p-0">
        <div className="flex max-h-[85vh] flex-col">
          <DialogHeader className="border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>Add agent to channel</DialogTitle>
            <DialogDescription>
              Add {agent?.name ?? "this agent"} to a channel so desktop chat can
              `@mention` it. Running agents are restarted automatically when
              they join a new channel so the harness picks up the new
              subscription immediately.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-5 px-6 py-5">
            <div className="space-y-1.5">
              <label className="text-sm font-medium" htmlFor="agent-channel-id">
                Channel
              </label>
              <select
                className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm"
                disabled={
                  channels.length === 0 || attachAgentMutation.isPending
                }
                id="agent-channel-id"
                onChange={(event) => setChannelId(event.target.value)}
                value={channelId}
              >
                {channels.length === 0 ? (
                  <option value="">No channels available</option>
                ) : null}
                {channels.map((channel) => (
                  <option key={channel.id} value={channel.id}>
                    {channel.name} · {channel.visibility}
                  </option>
                ))}
              </select>
              <p className="text-xs text-muted-foreground">
                Only channels accessible to the current desktop user are shown
                here.
              </p>
            </div>

            <div className="space-y-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="agent-channel-role"
              >
                Role
              </label>
              <select
                className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm"
                disabled={attachAgentMutation.isPending}
                id="agent-channel-role"
                onChange={(event) =>
                  setRole(event.target.value as Exclude<ChannelRole, "owner">)
                }
                value={role}
              >
                <option value="bot">bot</option>
                <option value="member">member</option>
                <option value="guest">guest</option>
                <option value="admin">admin</option>
              </select>
            </div>

            <div className="rounded-2xl border border-border/70 bg-muted/20 p-4">
              <p className="text-sm font-semibold tracking-tight">
                Agent pubkey
              </p>
              <div className="mt-3 flex items-center justify-between gap-3">
                <code className="min-w-0 flex-1 break-all rounded-xl border border-border/70 bg-background/80 px-3 py-2 text-xs">
                  {agent?.pubkey ?? "No agent selected"}
                </code>
                {agent ? (
                  <CopyButton label="Copy pubkey" value={agent.pubkey} />
                ) : null}
              </div>
            </div>

            {channelsQuery.error instanceof Error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {channelsQuery.error.message}
              </p>
            ) : null}

            {attachAgentMutation.error instanceof Error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {attachAgentMutation.error.message}
              </p>
            ) : null}
          </div>

          <div className="flex justify-end gap-2 border-t border-border/60 px-6 py-4">
            <Button
              onClick={() => handleOpenChange(false)}
              size="sm"
              type="button"
              variant="outline"
            >
              Cancel
            </Button>
            <Button
              disabled={
                !agent ||
                !selectedChannel ||
                channelsQuery.isLoading ||
                attachAgentMutation.isPending
              }
              onClick={() => void handleSubmit()}
              size="sm"
              type="button"
            >
              {attachAgentMutation.isPending ? "Adding..." : "Add to channel"}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
