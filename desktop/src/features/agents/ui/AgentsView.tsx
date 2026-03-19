import * as React from "react";

import {
  type AttachManagedAgentToChannelResult,
  useDeleteManagedAgentMutation,
  useManagedAgentLogQuery,
  useManagedAgentsQuery,
  useMintManagedAgentTokenMutation,
  useRelayAgentsQuery,
  useSetManagedAgentStartOnAppLaunchMutation,
  useStartManagedAgentMutation,
  useStopManagedAgentMutation,
} from "@/features/agents/hooks";
import type {
  Channel,
  CreateManagedAgentResponse,
  ManagedAgent,
} from "@/shared/api/types";
import { AddAgentToChannelDialog } from "./AddAgentToChannelDialog";
import { CreateAgentDialog } from "./CreateAgentDialog";
import { ManagedAgentLogPanel } from "./ManagedAgentLogPanel";
import { ManagedAgentsSection } from "./ManagedAgentsSection";
import { RelayDirectorySection } from "./RelayDirectorySection";
import { SecretRevealDialog } from "./SecretRevealDialog";
import { TokenRevealDialog } from "./TokenRevealDialog";

export function AgentsView() {
  const relayAgentsQuery = useRelayAgentsQuery();
  const managedAgentsQuery = useManagedAgentsQuery();
  const startMutation = useStartManagedAgentMutation();
  const stopMutation = useStopManagedAgentMutation();
  const startOnLaunchMutation = useSetManagedAgentStartOnAppLaunchMutation();
  const deleteMutation = useDeleteManagedAgentMutation();
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
  const [actionNoticeMessage, setActionNoticeMessage] = React.useState<
    string | null
  >(null);
  const [actionErrorMessage, setActionErrorMessage] = React.useState<
    string | null
  >(null);
  const managedAgents = React.useMemo(
    () =>
      [...(managedAgentsQuery.data ?? [])].sort((left, right) => {
        if (left.status !== right.status) {
          return left.status === "running" ? -1 : 1;
        }

        return left.name.localeCompare(right.name);
      }),
    [managedAgentsQuery.data],
  );
  const [logAgentPubkey, setLogAgentPubkey] = React.useState<string | null>(
    null,
  );
  const logAgent =
    managedAgents.find((agent) => agent.pubkey === logAgentPubkey) ?? null;
  const managedAgentLogQuery = useManagedAgentLogQuery(logAgentPubkey);
  const managedPubkeys = React.useMemo(
    () => new Set(managedAgents.map((agent) => agent.pubkey)),
    [managedAgents],
  );

  // Clear log selection if the agent was removed
  React.useEffect(() => {
    if (
      logAgentPubkey &&
      !managedAgents.some((agent) => agent.pubkey === logAgentPubkey)
    ) {
      setLogAgentPubkey(null);
    }
  }, [managedAgents, logAgentPubkey]);

  async function handleStart(pubkey: string) {
    setActionNoticeMessage(null);
    setActionErrorMessage(null);

    try {
      await startMutation.mutateAsync(pubkey);
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to start agent.",
      );
    }
  }

  async function handleStop(pubkey: string) {
    setActionNoticeMessage(null);
    setActionErrorMessage(null);

    try {
      await stopMutation.mutateAsync(pubkey);
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to stop agent.",
      );
    }
  }

  async function handleDelete(pubkey: string) {
    setActionNoticeMessage(null);
    setActionErrorMessage(null);

    try {
      await deleteMutation.mutateAsync(pubkey);
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
    setActionNoticeMessage(null);
    setActionErrorMessage(null);

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
    setActionNoticeMessage(null);
    setActionErrorMessage(null);

    try {
      const result = await mintTokenMutation.mutateAsync({
        pubkey,
        tokenName: `${name}-token`,
      });
      setRevealedToken({
        name,
        token: result.token,
      });
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

  function handleRefresh() {
    void managedAgentsQuery.refetch();
    void relayAgentsQuery.refetch();
    void managedAgentLogQuery.refetch();
  }

  const isActionPending =
    startMutation.isPending ||
    stopMutation.isPending ||
    startOnLaunchMutation.isPending ||
    deleteMutation.isPending ||
    mintTokenMutation.isPending;

  return (
    <>
      <div className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-4 sm:px-6">
        <div className="mx-auto flex w-full max-w-6xl flex-col gap-6">
          <div className="flex flex-col gap-6">
            <ManagedAgentsSection
              actionErrorMessage={actionErrorMessage}
              actionNoticeMessage={actionNoticeMessage}
              agents={managedAgents}
              error={
                managedAgentsQuery.error instanceof Error
                  ? managedAgentsQuery.error
                  : null
              }
              isActionPending={isActionPending}
              isLoading={managedAgentsQuery.isLoading}
              onAddToChannel={(agent) => {
                setActionNoticeMessage(null);
                setActionErrorMessage(null);
                setAgentToAddToChannel(agent);
              }}
              onCreate={() => {
                setIsCreateOpen(true);
              }}
              onDelete={(pubkey) => {
                void handleDelete(pubkey);
              }}
              onMintToken={(pubkey, name) => {
                void handleMintToken(pubkey, name);
              }}
              onRefresh={handleRefresh}
              onStart={(pubkey) => {
                void handleStart(pubkey);
              }}
              onStop={(pubkey) => {
                void handleStop(pubkey);
              }}
              onToggleStartOnAppLaunch={(pubkey, startOnAppLaunch) => {
                void handleToggleStartOnAppLaunch(pubkey, startOnAppLaunch);
              }}
              onViewLogs={setLogAgentPubkey}
            />

            <RelayDirectorySection
              error={
                relayAgentsQuery.error instanceof Error
                  ? relayAgentsQuery.error
                  : null
              }
              isLoading={relayAgentsQuery.isLoading}
              managedPubkeys={managedPubkeys}
              relayAgents={relayAgentsQuery.data ?? []}
            />
          </div>

          <ManagedAgentLogPanel
            error={
              managedAgentLogQuery.error instanceof Error
                ? managedAgentLogQuery.error
                : null
            }
            isLoading={managedAgentLogQuery.isLoading}
            logContent={managedAgentLogQuery.data?.content ?? null}
            selectedAgent={logAgent}
          />
        </div>
      </div>

      <CreateAgentDialog
        onCreated={(result) => {
          setLogAgentPubkey(result.agent.pubkey);
          setCreatedAgent(result);
        }}
        onOpenChange={setIsCreateOpen}
        open={isCreateOpen}
      />
      <AddAgentToChannelDialog
        agent={agentToAddToChannel}
        onAdded={handleAddedToChannel}
        onOpenChange={(open) => {
          if (!open) {
            setAgentToAddToChannel(null);
          }
        }}
        open={agentToAddToChannel !== null}
      />
      <SecretRevealDialog
        created={createdAgent}
        onOpenChange={(open) => {
          if (!open) {
            setCreatedAgent(null);
          }
        }}
      />
      <TokenRevealDialog
        name={revealedToken?.name ?? null}
        onOpenChange={(open) => {
          if (!open) {
            setRevealedToken(null);
          }
        }}
        token={revealedToken?.token ?? null}
      />
    </>
  );
}
