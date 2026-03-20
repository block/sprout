import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import {
  type AttachManagedAgentToChannelResult,
  personasQueryKey,
  useCreatePersonaMutation,
  useDeletePersonaMutation,
  useDeleteManagedAgentMutation,
  useExportPersonaPngMutation,
  useManagedAgentLogQuery,
  useManagedAgentsQuery,
  useMintManagedAgentTokenMutation,
  usePersonasQuery,
  useRelayAgentsQuery,
  useSetManagedAgentStartOnAppLaunchMutation,
  useStartManagedAgentMutation,
  useStopManagedAgentMutation,
  useUpdatePersonaMutation,
} from "@/features/agents/hooks";
import { useChannelsQuery } from "@/features/channels/hooks";
import { usePresenceQuery } from "@/features/presence/hooks";
import { sendChannelMessage } from "@/shared/api/tauri";
import type { ParsePersonaFilesResult } from "@/shared/api/tauriPersonas";
import type {
  AgentPersona,
  Channel,
  CreatePersonaInput,
  CreateManagedAgentResponse,
  ManagedAgent,
  UpdatePersonaInput,
} from "@/shared/api/types";
import { AddAgentToChannelDialog } from "./AddAgentToChannelDialog";
import { AddTeamToChannelDialog } from "./AddTeamToChannelDialog";
import { BatchImportDialog } from "./BatchImportDialog";
import { CreateAgentDialog } from "./CreateAgentDialog";
import { ManagedAgentLogPanel } from "./ManagedAgentLogPanel";
import { ManagedAgentsSection } from "./ManagedAgentsSection";
import { PersonaDialog } from "./PersonaDialog";
import { PersonaDeleteDialog } from "./PersonaDeleteDialog";
import { PersonasSection } from "./PersonasSection";
import { RelayDirectorySection } from "./RelayDirectorySection";
import { SecretRevealDialog } from "./SecretRevealDialog";
import { TeamDeleteDialog } from "./TeamDeleteDialog";
import { TeamDialog } from "./TeamDialog";
import { TeamsSection } from "./TeamsSection";
import { TokenRevealDialog } from "./TokenRevealDialog";
import { useTeamActions } from "./useTeamActions";

type PersonaDialogState = {
  description: string;
  enableImportDrop: boolean;
  initialValues: CreatePersonaInput | UpdatePersonaInput;
  submitLabel: string;
  title: string;
} | null;

export function AgentsView() {
  const queryClient = useQueryClient();
  const relayAgentsQuery = useRelayAgentsQuery();
  const managedAgentsQuery = useManagedAgentsQuery();
  const channelsQuery = useChannelsQuery();
  const personasQuery = usePersonasQuery();
  const startMutation = useStartManagedAgentMutation();
  const stopMutation = useStopManagedAgentMutation();
  const startOnLaunchMutation = useSetManagedAgentStartOnAppLaunchMutation();
  const deleteMutation = useDeleteManagedAgentMutation();
  const mintTokenMutation = useMintManagedAgentTokenMutation();
  const createPersonaMutation = useCreatePersonaMutation();
  const updatePersonaMutation = useUpdatePersonaMutation();
  const deletePersonaMutation = useDeletePersonaMutation();
  const exportPersonaPngMutation = useExportPersonaPngMutation();
  const [isCreateOpen, setIsCreateOpen] = React.useState(false);
  const [personaDialogState, setPersonaDialogState] =
    React.useState<PersonaDialogState>(null);
  const [personaToDelete, setPersonaToDelete] =
    React.useState<AgentPersona | null>(null);
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

  const teamActions = useTeamActions(
    { setActionNoticeMessage, setActionErrorMessage },
    {
      refetchManagedAgents: () => void managedAgentsQuery.refetch(),
      refetchRelayAgents: () => void relayAgentsQuery.refetch(),
    },
  );
  const [batchImportResult, setBatchImportResult] =
    React.useState<ParsePersonaFilesResult | null>(null);
  const [batchImportFileName, setBatchImportFileName] = React.useState("");
  const managedAgents = React.useMemo(
    () =>
      [...(managedAgentsQuery.data ?? [])].sort((left, right) => {
        if (left.status !== right.status) {
          return left.status === "running" || left.status === "deployed"
            ? -1
            : 1;
        }

        return left.name.localeCompare(right.name);
      }),
    [managedAgentsQuery.data],
  );
  const personas = personasQuery.data ?? [];
  const personaLabelsById = React.useMemo(
    () =>
      Object.fromEntries(
        personas.map((persona) => [persona.id, persona.displayName]),
      ),
    [personas],
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
  const managedPubkeyList = React.useMemo(
    () => managedAgents.map((agent) => agent.pubkey),
    [managedAgents],
  );
  const managedPresenceQuery = usePresenceQuery(managedPubkeyList);

  /** Resolve a relay-agent's first channel UUID for sending !shutdown. */
  function resolveAgentChannelId(pubkey: string): string | null {
    const relayAgents = relayAgentsQuery.data ?? [];
    const relayAgent = relayAgents.find((ra) => ra.pubkey === pubkey);
    // Prefer channelIds (new relay with json_agg). Fall back to resolving
    // channel names via the channels query (old relay without channel_ids).
    if (relayAgent?.channelIds?.length) {
      return relayAgent.channelIds[0];
    }
    // Fallback: resolve channel name → UUID via the channels query.
    // Only use this when the match is unambiguous — if multiple channels
    // share the same name (e.g. across teams), we can't be sure which one
    // the agent is in, and sending !shutdown to the wrong channel would
    // silently miss the agent. Return null to surface the error to the user.
    const channelName = relayAgent?.channels?.[0];
    if (!channelName) return null;
    const channels = channelsQuery.data ?? [];
    const matches = channels.filter((ch) => ch.name === channelName);
    return matches.length === 1 ? matches[0].id : null;
  }

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
      const agent = managedAgents.find((a) => a.pubkey === pubkey);
      if (!agent) return;

      if (agent.backend.type === "provider") {
        // Remote agent: send !shutdown mention via relay REST API.
        const channelId = resolveAgentChannelId(pubkey);
        if (!channelId) {
          setActionErrorMessage("Cannot stop: agent is not in any channel");
          return;
        }
        await sendChannelMessage(channelId, "!shutdown", undefined, undefined, [
          pubkey,
        ]);
        setActionNoticeMessage(
          "Shutdown command sent. Agent will stop shortly.",
        );
      } else {
        // Local agent: existing stop flow
        await stopMutation.mutateAsync(pubkey);
      }
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
      // For remote agents, send !shutdown before deleting to avoid orphaning.
      const agent = managedAgents.find((a) => a.pubkey === pubkey);
      if (agent?.backend.type === "provider" && agent.backendAgentId) {
        const presence =
          managedPresenceQuery.data?.[pubkey.trim().toLowerCase()];
        const channelId = resolveAgentChannelId(pubkey);
        if (channelId) {
          // If the agent is still online, send !shutdown and warn that
          // deletion proceeds without waiting for confirmed exit.
          if (presence === "online" || presence === "away") {
            await sendChannelMessage(
              channelId,
              "!shutdown",
              undefined,
              undefined,
              [pubkey],
            );
            // eslint-disable-next-line no-alert
            const confirmed = window.confirm(
              "Shutdown command sent, but the agent may still be running. " +
                "Deleting now removes the local record — the remote deployment " +
                "will be orphaned if shutdown hasn't completed. Continue?",
            );
            if (!confirmed) return;
          } else {
            // Offline presence means the process isn't connected, but the
            // remote infrastructure (VM/container) may still exist. Confirm
            // before removing the local record — it's the only management handle.
            // eslint-disable-next-line no-alert
            const confirmed = window.confirm(
              "This agent is offline but the remote deployment may still exist. " +
                "Deleting removes the local management record. Continue?",
            );
            if (!confirmed) return;
          }
        } else {
          // Can't send shutdown — warn user about orphaning.
          // eslint-disable-next-line no-alert
          const confirmed = window.confirm(
            "This agent is deployed but not in any channel. " +
              "Deleting will orphan the remote deployment (it will keep running). Continue?",
          );
          if (!confirmed) return;
        }
      }
      // Pass forceRemoteDelete for deployed provider agents — the backend
      // rejects deletion of deployed remote agents without this flag.
      const isDeployedRemote =
        agent?.backend.type === "provider" && agent?.backendAgentId;
      await deleteMutation.mutateAsync({
        pubkey,
        forceRemoteDelete: isDeployedRemote ? true : undefined,
      });
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

  async function handlePersonaSubmit(
    input: CreatePersonaInput | UpdatePersonaInput,
  ) {
    setActionNoticeMessage(null);
    setActionErrorMessage(null);

    try {
      if ("id" in input) {
        await updatePersonaMutation.mutateAsync(input);
        setActionNoticeMessage(`Updated ${input.displayName}.`);
      } else {
        await createPersonaMutation.mutateAsync(input);
        setActionNoticeMessage(`Created ${input.displayName}.`);
      }
      setPersonaDialogState(null);
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to save persona.",
      );
    }
  }

  async function handleDeletePersona(persona: AgentPersona) {
    setActionNoticeMessage(null);
    setActionErrorMessage(null);

    try {
      await deletePersonaMutation.mutateAsync(persona.id);
      setActionNoticeMessage(`Deleted ${persona.displayName}.`);
      setPersonaToDelete(null);
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to delete persona.",
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

  const isActionPending =
    startMutation.isPending ||
    stopMutation.isPending ||
    startOnLaunchMutation.isPending ||
    deleteMutation.isPending ||
    mintTokenMutation.isPending ||
    createPersonaMutation.isPending ||
    updatePersonaMutation.isPending ||
    deletePersonaMutation.isPending ||
    teamActions.createTeamMutation.isPending ||
    teamActions.updateTeamMutation.isPending ||
    teamActions.deleteTeamMutation.isPending;

  return (
    <>
      <div className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-4 sm:px-6">
        <div className="mx-auto flex w-full max-w-6xl flex-col gap-6">
          <div className="flex flex-col gap-6">
            <PersonasSection
              error={
                personasQuery.error instanceof Error
                  ? personasQuery.error
                  : null
              }
              isLoading={personasQuery.isLoading}
              isPending={
                createPersonaMutation.isPending ||
                updatePersonaMutation.isPending ||
                deletePersonaMutation.isPending
              }
              onCreate={() => {
                setActionNoticeMessage(null);
                setActionErrorMessage(null);
                setPersonaDialogState({
                  title: "Create persona",
                  description:
                    "Save a reusable role, prompt, and optional avatar for future agent deployments.",
                  enableImportDrop: true,
                  submitLabel: "Create persona",
                  initialValues: {
                    displayName: "",
                    avatarUrl: "",
                    systemPrompt: "",
                  },
                });
              }}
              onDelete={setPersonaToDelete}
              onDuplicate={(persona) => {
                setActionNoticeMessage(null);
                setActionErrorMessage(null);
                setPersonaDialogState({
                  title: `Duplicate ${persona.displayName}`,
                  description:
                    "Create a new persona by copying this template and adjusting it as needed.",
                  enableImportDrop: false,
                  submitLabel: "Create persona",
                  initialValues: {
                    displayName: `${persona.displayName} copy`,
                    avatarUrl: persona.avatarUrl ?? "",
                    systemPrompt: persona.systemPrompt,
                  },
                });
              }}
              onEdit={(persona) => {
                setActionNoticeMessage(null);
                setActionErrorMessage(null);
                setPersonaDialogState({
                  title: `Edit ${persona.displayName}`,
                  description:
                    "Update this saved persona. New deployments will use the updated values.",
                  enableImportDrop: false,
                  submitLabel: "Save changes",
                  initialValues: {
                    id: persona.id,
                    displayName: persona.displayName,
                    avatarUrl: persona.avatarUrl ?? "",
                    systemPrompt: persona.systemPrompt,
                  },
                });
              }}
              onExport={(persona) => {
                exportPersonaPngMutation.mutate(persona.id, {
                  onSuccess: (saved) => {
                    if (saved) {
                      setActionNoticeMessage(
                        `Exported ${persona.displayName}.`,
                      );
                    }
                  },
                  onError: (error) => {
                    setActionErrorMessage(
                      error instanceof Error
                        ? error.message
                        : "Failed to export persona.",
                    );
                  },
                });
              }}
              personas={personas}
            />

            <TeamsSection
              error={
                teamActions.teamsQuery.error instanceof Error
                  ? teamActions.teamsQuery.error
                  : null
              }
              isLoading={teamActions.teamsQuery.isLoading}
              isPending={
                teamActions.createTeamMutation.isPending ||
                teamActions.updateTeamMutation.isPending ||
                teamActions.deleteTeamMutation.isPending
              }
              onCreate={teamActions.openCreateDialog}
              onDelete={teamActions.setTeamToDelete}
              onDuplicate={teamActions.openDuplicateDialog}
              onEdit={teamActions.openEditDialog}
              onAddToChannel={teamActions.setTeamToAddToChannel}
              personas={personas}
              teams={teamActions.teams}
            />

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
              personaLabelsById={personaLabelsById}
              presenceLookup={managedPresenceQuery.data ?? {}}
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
      <PersonaDialog
        description={personaDialogState?.description ?? ""}
        enableImportDrop={personaDialogState?.enableImportDrop ?? false}
        error={
          updatePersonaMutation.error instanceof Error
            ? updatePersonaMutation.error
            : createPersonaMutation.error instanceof Error
              ? createPersonaMutation.error
              : null
        }
        initialValues={personaDialogState?.initialValues ?? null}
        isPending={
          createPersonaMutation.isPending || updatePersonaMutation.isPending
        }
        onBatchImport={(result, fileName) => {
          setBatchImportResult(result);
          setBatchImportFileName(fileName);
          setPersonaDialogState(null);
        }}
        onOpenChange={(open) => {
          if (!open) {
            setPersonaDialogState(null);
          }
        }}
        onSubmit={handlePersonaSubmit}
        open={personaDialogState !== null}
        submitLabel={personaDialogState?.submitLabel ?? "Save"}
        title={personaDialogState?.title ?? "Persona"}
      />
      <PersonaDeleteDialog
        onConfirm={(persona) => {
          void handleDeletePersona(persona);
        }}
        onOpenChange={(open) => {
          if (!open) {
            setPersonaToDelete(null);
          }
        }}
        open={personaToDelete !== null}
        persona={personaToDelete}
      />
      <TeamDialog
        description={teamActions.teamDialogState?.description ?? ""}
        error={
          teamActions.updateTeamMutation.error instanceof Error
            ? teamActions.updateTeamMutation.error
            : teamActions.createTeamMutation.error instanceof Error
              ? teamActions.createTeamMutation.error
              : null
        }
        initialValues={teamActions.teamDialogState?.initialValues ?? null}
        isPending={
          teamActions.createTeamMutation.isPending ||
          teamActions.updateTeamMutation.isPending
        }
        onOpenChange={(open) => {
          if (!open) {
            teamActions.setTeamDialogState(null);
          }
        }}
        onSubmit={teamActions.handleTeamSubmit}
        open={teamActions.teamDialogState !== null}
        personas={personas}
        submitLabel={teamActions.teamDialogState?.submitLabel ?? "Save"}
        title={teamActions.teamDialogState?.title ?? "Team"}
      />
      <TeamDeleteDialog
        onConfirm={(team) => {
          void teamActions.handleDeleteTeam(team);
        }}
        onOpenChange={(open) => {
          if (!open) {
            teamActions.setTeamToDelete(null);
          }
        }}
        open={teamActions.teamToDelete !== null}
        team={teamActions.teamToDelete}
      />
      <AddTeamToChannelDialog
        onDeployed={teamActions.handleTeamDeployed}
        onOpenChange={(open) => {
          if (!open) {
            teamActions.setTeamToAddToChannel(null);
          }
        }}
        open={teamActions.teamToAddToChannel !== null}
        personas={personas}
        team={teamActions.teamToAddToChannel}
      />
      <BatchImportDialog
        fileName={batchImportFileName}
        onComplete={(count) => {
          setBatchImportResult(null);
          setActionNoticeMessage(
            `Imported ${count} persona${count !== 1 ? "s" : ""}.`,
          );
          void queryClient.invalidateQueries({ queryKey: personasQueryKey });
        }}
        onOpenChange={(open) => {
          if (!open) {
            setBatchImportResult(null);
          }
        }}
        open={batchImportResult !== null}
        result={batchImportResult}
      />
    </>
  );
}
