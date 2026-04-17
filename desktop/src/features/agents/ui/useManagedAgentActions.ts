import * as React from "react";

import {
  type AttachManagedAgentToChannelResult,
  useManagedAgentLogQuery,
  useManagedAgentsQuery,
  useMintManagedAgentTokenMutation,
  useRelayAgentsQuery,
  useSetManagedAgentStartOnAppLaunchMutation,
  useStartManagedAgentMutation,
  useStopManagedAgentMutation,
  useDeleteManagedAgentMutation,
} from "@/features/agents/hooks";
import { useChannelsQuery } from "@/features/channels/hooks";
import { usePresenceQuery } from "@/features/presence/hooks";
import type {
  Channel,
  CreateManagedAgentResponse,
  ManagedAgent,
} from "@/shared/api/types";
import {
  deleteManagedAgentWithRules,
  startManagedAgentWithRules,
  stopManagedAgentWithRules,
} from "../lib/managedAgentControlActions";

export function useManagedAgentActions() {
  const relayAgentsQuery = useRelayAgentsQuery();
  const managedAgentsQuery = useManagedAgentsQuery();
  const channelsQuery = useChannelsQuery();
  const startMutation = useStartManagedAgentMutation();
  const stopMutation = useStopManagedAgentMutation();
  const deleteMutation = useDeleteManagedAgentMutation();
  const startOnLaunchMutation = useSetManagedAgentStartOnAppLaunchMutation();
  const mintTokenMutation = useMintManagedAgentTokenMutation();

  const [isCreateOpen, setIsCreateOpen] = React.useState(false);
  const [agentToAddToChannel, setAgentToAddToChannel] =
    React.useState<ManagedAgent | null>(null);
  const [createdAgent, setCreatedAgent] =
    React.useState<CreateManagedAgentResponse | null>(null);
  const [revealedToken, setRevealedToken] = React.useState<{
    name: string;
    token: string;
  } | null>(null);
  const [logAgentPubkey, setLogAgentPubkey] = React.useState<string | null>(
    null,
  );
  const [actionNoticeMessage, setActionNoticeMessage] = React.useState<
    string | null
  >(null);
  const [actionErrorMessage, setActionErrorMessage] = React.useState<
    string | null
  >(null);

  const managedAgentLogQuery = useManagedAgentLogQuery(logAgentPubkey);

  const managedAgents = React.useMemo(
    () =>
      [...(managedAgentsQuery.data ?? [])].sort((left, right) => {
        const activeScore = (s: string) =>
          s === "running" || s === "deployed" ? 1 : 0;
        const diff = activeScore(right.status) - activeScore(left.status);
        if (diff !== 0) return diff;
        return left.name.localeCompare(right.name);
      }),
    [managedAgentsQuery.data],
  );

  const managedPubkeys = React.useMemo(
    () => new Set(managedAgents.map((agent) => agent.pubkey)),
    [managedAgents],
  );

  const managedPubkeyList = React.useMemo(
    () => managedAgents.map((agent) => agent.pubkey),
    [managedAgents],
  );

  const managedPresenceQuery = usePresenceQuery(managedPubkeyList);

  const channelsByPubkey = React.useMemo(() => {
    const map: Record<string, string[]> = {};
    for (const ra of relayAgentsQuery.data ?? []) {
      if (ra.channels.length > 0) {
        map[ra.pubkey] = ra.channels;
      }
    }
    return map;
  }, [relayAgentsQuery.data]);

  // Clear log selection if the agent was removed
  React.useEffect(() => {
    if (
      logAgentPubkey &&
      !managedAgents.some((agent) => agent.pubkey === logAgentPubkey)
    ) {
      setLogAgentPubkey(null);
    }
  }, [managedAgents, logAgentPubkey]);

  function clearFeedback() {
    setActionNoticeMessage(null);
    setActionErrorMessage(null);
  }

  async function handleStart(pubkey: string) {
    clearFeedback();
    try {
      const agent = managedAgents.find((c) => c.pubkey === pubkey);
      if (!agent) return;
      await startManagedAgentWithRules({
        agent,
        startManagedAgent: startMutation.mutateAsync,
      });
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to start agent.",
      );
    }
  }

  async function handleStop(pubkey: string) {
    clearFeedback();
    try {
      const agent = managedAgents.find((a) => a.pubkey === pubkey);
      if (!agent) return;
      const result = await stopManagedAgentWithRules({
        agent,
        channels: channelsQuery.data ?? [],
        relayAgents: relayAgentsQuery.data ?? [],
        stopManagedAgent: stopMutation.mutateAsync,
      });
      if (result.noticeMessage) {
        setActionNoticeMessage(result.noticeMessage);
      }
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to stop agent.",
      );
    }
  }

  async function handleDelete(pubkey: string) {
    clearFeedback();
    try {
      const agent = managedAgents.find((a) => a.pubkey === pubkey);
      if (!agent) return;
      const result = await deleteManagedAgentWithRules({
        agent,
        channels: channelsQuery.data ?? [],
        deleteManagedAgent: deleteMutation.mutateAsync,
        presenceLookup: managedPresenceQuery.data,
        relayAgents: relayAgentsQuery.data ?? [],
      });
      if (result.cancelled) return;
      if (logAgentPubkey === pubkey) {
        setLogAgentPubkey(null);
      }
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to delete agent.",
      );
    }
  }

  async function handleToggleStartOnAppLaunch(
    pubkey: string,
    startOnAppLaunch: boolean,
  ) {
    clearFeedback();
    try {
      const updated = await startOnLaunchMutation.mutateAsync({
        pubkey,
        startOnAppLaunch,
      });
      setActionNoticeMessage(
        updated.startOnAppLaunch
          ? `Will start ${updated.name} automatically when the desktop app opens.`
          : `${updated.name} will stay manual-start only.`,
      );
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error
          ? error.message
          : "Failed to update startup preference.",
      );
    }
  }

  async function handleMintToken(pubkey: string, name: string) {
    clearFeedback();
    try {
      const result = await mintTokenMutation.mutateAsync({
        pubkey,
        tokenName: `${name}-token`,
      });
      setRevealedToken({ name, token: result.token });
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to mint token.",
      );
    }
  }

  function handleAddedToChannel(
    channel: Channel,
    result: AttachManagedAgentToChannelResult,
  ) {
    setActionErrorMessage(null);
    setActionNoticeMessage(() => {
      if (result.restarted) {
        return `Added ${result.agent.name} to ${channel.name} and restarted it so the new channel subscription is live.`;
      }
      if (result.started) {
        return `Added ${result.agent.name} to ${channel.name} and spawned it.`;
      }
      if (result.membershipAdded) {
        return `Added ${result.agent.name} to ${channel.name}.`;
      }
      return `${result.agent.name} is already in ${channel.name}.`;
    });
    void managedAgentsQuery.refetch();
    void relayAgentsQuery.refetch();
  }

  const isPending =
    startMutation.isPending ||
    stopMutation.isPending ||
    startOnLaunchMutation.isPending ||
    deleteMutation.isPending ||
    mintTokenMutation.isPending;

  return {
    // Queries
    relayAgentsQuery,
    managedAgentsQuery,
    managedAgentLogQuery,
    managedPresenceQuery,
    // Derived state
    managedAgents,
    managedPubkeys,
    channelsByPubkey,
    isPending,
    // UI state
    isCreateOpen,
    setIsCreateOpen,
    agentToAddToChannel,
    setAgentToAddToChannel,
    createdAgent,
    setCreatedAgent,
    revealedToken,
    setRevealedToken,
    logAgentPubkey,
    setLogAgentPubkey,
    actionNoticeMessage,
    setActionNoticeMessage,
    actionErrorMessage,
    setActionErrorMessage,
    // Handlers
    handleStart,
    handleStop,
    handleDelete,
    handleToggleStartOnAppLaunch,
    handleMintToken,
    handleAddedToChannel,
    // Refetch helpers (for cross-domain use)
    refetchManagedAgents: () => void managedAgentsQuery.refetch(),
    refetchRelayAgents: () => void relayAgentsQuery.refetch(),
  };
}
