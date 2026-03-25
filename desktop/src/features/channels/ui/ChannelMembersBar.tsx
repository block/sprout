import { Bot, Plus, Settings2, UserRound } from "lucide-react";
import * as React from "react";

import {
  useAcpProvidersQuery,
  useBackendProvidersQuery,
  useManagedAgentsQuery,
  useRelayAgentsQuery,
} from "@/features/agents/hooks";
import { useChannelMembersQuery } from "@/features/channels/hooks";
import type { Channel } from "@/shared/api/types";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { Button } from "@/shared/ui/button";
import { AddChannelBotDialog } from "./AddChannelBotDialog";

type ChannelMembersBarProps = {
  channel: Channel;
  currentPubkey?: string;
  onManageChannel: () => void;
};

function CountStat({
  count,
  icon: Icon,
  label,
  loading,
}: {
  count: number;
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  loading?: boolean;
}) {
  return (
    <div className="inline-flex h-8 items-center justify-center gap-1.5 rounded-full bg-muted/55 px-2.5 text-muted-foreground">
      <Icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground/70" />
      <span className="relative top-px min-w-[1ch] text-[13px] font-medium leading-none text-muted-foreground/80 tabular-nums">
        {loading ? "..." : count}
      </span>
      <span className="sr-only">{loading ? `Loading ${label}` : label}</span>
    </div>
  );
}

export function ChannelMembersBar({
  channel,
  currentPubkey,
  onManageChannel,
}: ChannelMembersBarProps) {
  const [isAddBotOpen, setIsAddBotOpen] = React.useState(false);
  const membersQuery = useChannelMembersQuery(channel.id);
  const providersQuery = useAcpProvidersQuery();
  const backendProvidersQuery = useBackendProvidersQuery();
  const managedAgentsQuery = useManagedAgentsQuery();
  const relayAgentsQuery = useRelayAgentsQuery();
  const members = membersQuery.data ?? [];
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
  const managedAgents = managedAgentsQuery.data ?? [];
  const relayAgents = relayAgentsQuery.data ?? [];
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
  const managedAgentPubkeys = React.useMemo(
    () => new Set(managedAgents.map((agent) => normalizePubkey(agent.pubkey))),
    [managedAgents],
  );
  const relayAgentPubkeys = React.useMemo(
    () => new Set(relayAgents.map((agent) => normalizePubkey(agent.pubkey))),
    [relayAgents],
  );
  const peopleCount = React.useMemo(
    () =>
      members.filter((member) => {
        const normalizedPubkey = normalizePubkey(member.pubkey);
        return (
          member.role !== "bot" &&
          !managedAgentPubkeys.has(normalizedPubkey) &&
          !relayAgentPubkeys.has(normalizedPubkey)
        );
      }).length,
    [managedAgentPubkeys, members, relayAgentPubkeys],
  );
  const botCount = members.length - peopleCount;
  const previousChannelIdRef = React.useRef(channel.id);

  React.useEffect(() => {
    if (previousChannelIdRef.current === channel.id) {
      return;
    }

    previousChannelIdRef.current = channel.id;
    setIsAddBotOpen(false);
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
        <CountStat
          count={peopleCount}
          icon={UserRound}
          label="people"
          loading={membersQuery.isLoading}
        />

        <CountStat
          count={botCount}
          icon={Bot}
          label="bots"
          loading={membersQuery.isLoading}
        />

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
