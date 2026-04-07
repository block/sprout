import { Plus, Settings2, Users, Zap } from "lucide-react";
import * as React from "react";

import {
  useAcpProvidersQuery,
  useBackendProvidersQuery,
  useManagedAgentsQuery,
  useRelayAgentsQuery,
} from "@/features/agents/hooks";
import { useChannelMembersQuery } from "@/features/channels/hooks";
import { CreateWorkflowDialog } from "@/features/workflows/ui/CreateWorkflowDialog";
import type { Channel } from "@/shared/api/types";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { Button } from "@/shared/ui/button";
import { AddChannelBotDialog } from "./AddChannelBotDialog";

type ChannelMembersBarProps = {
  channel: Channel;
  currentPubkey?: string;
  onManageChannel: () => void;
  onToggleMembers: () => void;
};

export function ChannelMembersBar({
  channel,
  currentPubkey,
  onManageChannel,
  onToggleMembers,
}: ChannelMembersBarProps) {
  const [isAddBotOpen, setIsAddBotOpen] = React.useState(false);
  const [isCreateWorkflowOpen, setIsCreateWorkflowOpen] = React.useState(false);
  const membersQuery = useChannelMembersQuery(channel.id);
  const providersQuery = useAcpProvidersQuery();
  const backendProvidersQuery = useBackendProvidersQuery();
  const managedAgentsQuery = useManagedAgentsQuery();
  const relayAgentsQuery = useRelayAgentsQuery();
  const members = membersQuery.data ?? [];
  const memberCount = membersQuery.data?.length ?? channel.memberCount;
  const providers = React.useMemo(
    () =>
      [...(providersQuery.data ?? [])].sort((left, right) => {
        const leftPriority = left.id === "goose" ? 0 : 1;
        const rightPriority = right.id === "goose" ? 0 : 1;
        if (leftPriority !== rightPriority) {
          return leftPriority - rightPriority;
        }

        return left.label.localeCompare(right.label);
      }),
    [providersQuery.data],
  );
  const normalizedCurrentPubkey = currentPubkey
    ? normalizePubkey(currentPubkey)
    : null;
  const selfMember =
    members.find(
      (member) => normalizePubkey(member.pubkey) === normalizedCurrentPubkey,
    ) ?? null;
  const canManageMembers =
    selfMember?.role === "owner" || selfMember?.role === "admin";
  const canAddAgents =
    channel.channelType !== "dm" &&
    channel.archivedAt === null &&
    (channel.visibility === "open" || canManageMembers);
  const previousChannelIdRef = React.useRef(channel.id);

  React.useEffect(() => {
    if (previousChannelIdRef.current === channel.id) {
      return;
    }

    previousChannelIdRef.current = channel.id;
    setIsAddBotOpen(false);
    setIsCreateWorkflowOpen(false);
  }, [channel.id]);

  const dialogErrorMessage =
    providersQuery.error instanceof Error
      ? providersQuery.error.message
      : managedAgentsQuery.error instanceof Error
        ? managedAgentsQuery.error.message
        : relayAgentsQuery.error instanceof Error
          ? relayAgentsQuery.error.message
          : null;

  return (
    <React.Fragment>
      <div className="flex items-center gap-2">
        <Button
          aria-label="Add agent"
          className="h-9 w-9 rounded-full"
          data-testid="channel-add-bot-trigger"
          disabled={!canAddAgents}
          onClick={() => {
            setIsAddBotOpen(true);
          }}
          size="icon"
          type="button"
          variant="outline"
        >
          <Plus className="h-4 w-4" />
        </Button>

        <Button
          aria-label="Create workflow"
          className="h-9 w-9 rounded-full"
          data-testid="channel-create-workflow-trigger"
          disabled={!canAddAgents}
          onClick={() => {
            setIsCreateWorkflowOpen(true);
          }}
          size="icon"
          type="button"
          variant="outline"
        >
          <Zap className="h-4 w-4" />
        </Button>

        <Button
          aria-label={`View channel members (${memberCount})`}
          className="h-9 gap-1.5 rounded-full px-3"
          data-testid="channel-members-trigger"
          onClick={onToggleMembers}
          type="button"
          variant="outline"
        >
          <Users className="h-4 w-4" />
          <span className="min-w-[1ch] text-sm font-medium tabular-nums">
            {memberCount}
          </span>
        </Button>

        <Button
          aria-label="Manage channel"
          className="h-9 w-9 rounded-full"
          data-testid="channel-management-trigger"
          onClick={onManageChannel}
          size="icon"
          type="button"
          variant="outline"
        >
          <Settings2 className="h-4 w-4" />
        </Button>
      </div>

      <CreateWorkflowDialog
        channels={[channel]}
        onOpenChange={setIsCreateWorkflowOpen}
        open={isCreateWorkflowOpen}
      />

      <AddChannelBotDialog
        backendProviders={backendProvidersQuery.data ?? []}
        backendProvidersLoading={backendProvidersQuery.isLoading}
        channelId={channel.id}
        onOpenChange={setIsAddBotOpen}
        open={isAddBotOpen}
        providers={providers}
        providersErrorMessage={dialogErrorMessage}
        providersLoading={providersQuery.isLoading}
      />
    </React.Fragment>
  );
}
