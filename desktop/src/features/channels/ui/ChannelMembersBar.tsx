import { Plus, Settings2, Users, Zap } from "lucide-react";
import * as React from "react";

import {
  useAcpProvidersQuery,
  useBackendProvidersQuery,
  useManagedAgentsQuery,
  usePersonasQuery,
  useRelayAgentsQuery,
} from "@/features/agents/hooks";
import { pickBotName } from "@/features/agents/lib/pickBotName";
import {
  useBotRecents,
  DEFAULT_PERSONA_NAMES,
} from "@/features/agents/lib/useBotRecents";
import { useChannelMembersQuery } from "@/features/channels/hooks";
import { QuickBotBar } from "@/features/channels/ui/QuickBotBar";
import { useQuickBotDrop } from "@/features/channels/ui/useQuickBotDrop";
import { CreateWorkflowDialog } from "@/features/workflows/ui/CreateWorkflowDialog";
import type { AgentPersona, Channel } from "@/shared/api/types";
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
  const personasQuery2 = usePersonasQuery();
  const allPersonas = personasQuery2.data ?? [];
  const { recentIds, pushRecent } = useBotRecents();
  const quickDrop = useQuickBotDrop(channel.id);

  // Track in-flight instance names so rapid clicks don't produce duplicates.
  // Cleared when the members query refetches with the new member.
  const inflightNamesRef = React.useRef<Record<string, string[]>>({});

  // Resolve the 3 personas to show in the quick bar.
  // Use recents if available, otherwise fall back to default names.
  const quickPersonas = React.useMemo(() => {
    if (allPersonas.length === 0) return [];

    const resolved: typeof allPersonas = [];

    if (recentIds.length > 0) {
      for (const id of recentIds) {
        const found = allPersonas.find((p) => p.id === id);
        if (found) resolved.push(found);
        if (resolved.length >= 3) break;
      }
    }

    if (resolved.length < 3) {
      for (const name of DEFAULT_PERSONA_NAMES) {
        if (resolved.length >= 3) break;
        const found = allPersonas.find(
          (p) =>
            p.displayName.toLowerCase() === name.toLowerCase() &&
            !resolved.some((r) => r.id === p.id),
        );
        if (found) resolved.push(found);
      }
    }

    // Reset in-flight names when members list updates (the new bot appeared).
    inflightNamesRef.current = {};

    // Build the set of names already used in this channel
    const usedNames = new Set(
      members.map((m) => m.displayName ?? "").filter((n) => n.length > 0),
    );

    // Compute instance names from persona name pools
    return resolved.map((persona) => {
      // Include in-flight names to avoid duplicates on rapid clicks
      const inflight = inflightNamesRef.current[persona.id] ?? [];
      const combinedUsed = new Set(usedNames);
      for (const n of inflight) combinedUsed.add(n);

      const instanceName = pickBotName(persona.namePool ?? [], combinedUsed);
      return { persona, instanceName };
    });
  }, [allPersonas, recentIds, members]);

  const addBot = quickDrop.addBot;
  const handleQuickAdd = React.useCallback(
    async (persona: AgentPersona, instanceName: string) => {
      // Optimistically track the chosen name to avoid duplicates on rapid clicks.
      inflightNamesRef.current[persona.id] = [
        ...(inflightNamesRef.current[persona.id] ?? []),
        instanceName,
      ];
      pushRecent(persona.id);
      await addBot(persona, instanceName);
    },
    [pushRecent, addBot],
  );

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
        <div className="group/quick flex items-center">
          {canAddAgents ? (
            <QuickBotBar
              personas={quickPersonas}
              pending={quickDrop.pending}
              onAdd={handleQuickAdd}
            />
          ) : null}
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
        </div>

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
