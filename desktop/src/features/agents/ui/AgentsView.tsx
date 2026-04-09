import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import {
  type AttachManagedAgentToChannelResult,
  personasQueryKey,
  useAcpProvidersQuery,
  useCreatePersonaMutation,
  useDeletePersonaMutation,
  useDeleteManagedAgentMutation,
  useExportPersonaJsonMutation,
  useManagedAgentLogQuery,
  useManagedAgentsQuery,
  useMintManagedAgentTokenMutation,
  usePersonasQuery,
  useRelayAgentsQuery,
  useSetManagedAgentStartOnAppLaunchMutation,
  useSetPersonaActiveMutation,
  useStartManagedAgentMutation,
  useStopManagedAgentMutation,
  useUpdatePersonaMutation,
} from "@/features/agents/hooks";
import { getPersonaLibraryState } from "@/features/agents/lib/catalog";
import { useChannelsQuery } from "@/features/channels/hooks";
import { usePresenceQuery } from "@/features/presence/hooks";
import { sendChannelMessage } from "@/shared/api/tauri";
import {
  parsePersonaFiles,
  type ParsePersonaFilesResult,
} from "@/shared/api/tauriPersonas";
import { isSingleItemFile } from "@/shared/lib/fileMagic";

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
import { ManagedAgentsSection } from "./ManagedAgentsSection";
import { PersonaCatalogDialog } from "./PersonaCatalogDialog";
import { PersonaDialog } from "./PersonaDialog";
import { PersonaDeleteDialog } from "./PersonaDeleteDialog";
import { PersonasSection } from "./PersonasSection";
import { RelayDirectorySection } from "./RelayDirectorySection";
import { SecretRevealDialog } from "./SecretRevealDialog";
import { TeamDeleteDialog } from "./TeamDeleteDialog";
import { TeamDialog } from "./TeamDialog";
import { TeamImportDialog } from "./TeamImportDialog";
import { TeamsSection } from "./TeamsSection";
import { TokenRevealDialog } from "./TokenRevealDialog";
import {
  createPersonaDialogState,
  duplicatePersonaDialogState,
  editPersonaDialogState,
  importPersonaDialogState,
  type PersonaDialogState,
} from "./personaDialogState";
import { useTeamActions } from "./useTeamActions";

type PersonaFeedbackSurface = "catalog" | "library";

export function AgentsView() {
  const queryClient = useQueryClient();
  const relayAgentsQuery = useRelayAgentsQuery();
  const managedAgentsQuery = useManagedAgentsQuery();
  const channelsQuery = useChannelsQuery();
  const personasQuery = usePersonasQuery();
  const acpProvidersQuery = useAcpProvidersQuery();
  const startMutation = useStartManagedAgentMutation();
  const stopMutation = useStopManagedAgentMutation();
  const startOnLaunchMutation = useSetManagedAgentStartOnAppLaunchMutation();
  const deleteMutation = useDeleteManagedAgentMutation();
  const mintTokenMutation = useMintManagedAgentTokenMutation();
  const createPersonaMutation = useCreatePersonaMutation();
  const updatePersonaMutation = useUpdatePersonaMutation();
  const deletePersonaMutation = useDeletePersonaMutation();
  const setPersonaActiveMutation = useSetPersonaActiveMutation();
  const exportPersonaJsonMutation = useExportPersonaJsonMutation();
  const [isCreateOpen, setIsCreateOpen] = React.useState(false);
  const [personaDialogState, setPersonaDialogState] =
    React.useState<PersonaDialogState | null>(null);
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
  const [personaNoticeMessage, setPersonaNoticeMessage] = React.useState<
    string | null
  >(null);
  const [personaErrorMessage, setPersonaErrorMessage] = React.useState<
    string | null
  >(null);
  const [personaFeedbackSurface, setPersonaFeedbackSurface] =
    React.useState<PersonaFeedbackSurface>("library");

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
  const [isCatalogDialogOpen, setIsCatalogDialogOpen] = React.useState(false);
  const managedAgents = React.useMemo(
    () =>
      [...(managedAgentsQuery.data ?? [])].sort((left, right) => {
        // Active agents (running or deployed) sort before inactive ones.
        // Both "running" and "deployed" are equivalent for sorting purposes.
        const activeScore = (s: string) =>
          s === "running" || s === "deployed" ? 1 : 0;
        const diff = activeScore(right.status) - activeScore(left.status);
        if (diff !== 0) return diff;

        return left.name.localeCompare(right.name);
      }),
    [managedAgentsQuery.data],
  );
  const personas = personasQuery.data ?? [];
  const { catalogPersonas, libraryPersonas, personaLabelsById } = React.useMemo(
    () => getPersonaLibraryState(personas),
    [personas],
  );
  const [logAgentPubkey, setLogAgentPubkey] = React.useState<string | null>(
    null,
  );
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

  function clearActionFeedback() {
    setActionNoticeMessage(null);
    setActionErrorMessage(null);
  }

  function clearPersonaFeedback(
    surface: PersonaFeedbackSurface = personaFeedbackSurface,
  ) {
    setPersonaFeedbackSurface(surface);
    setPersonaNoticeMessage(null);
    setPersonaErrorMessage(null);
  }

  async function handleStart(pubkey: string) {
    clearActionFeedback();

    try {
      await startMutation.mutateAsync(pubkey);
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to start agent.",
      );
    }
  }

  async function handleStop(pubkey: string) {
    clearActionFeedback();

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
    clearActionFeedback();

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
    clearActionFeedback();

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
    clearActionFeedback();

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
    clearPersonaFeedback("library");

    try {
      if ("id" in input) {
        await updatePersonaMutation.mutateAsync(input);
        setPersonaNoticeMessage(`Updated ${input.displayName}.`);
      } else {
        await createPersonaMutation.mutateAsync(input);
        setPersonaNoticeMessage(`Created ${input.displayName}.`);
      }
      setPersonaDialogState(null);
    } catch (error) {
      setPersonaErrorMessage(
        error instanceof Error ? error.message : "Failed to save persona.",
      );
    }
  }

  async function handleDeletePersona(persona: AgentPersona) {
    clearPersonaFeedback("library");

    try {
      await deletePersonaMutation.mutateAsync(persona.id);
      setPersonaNoticeMessage(`Deleted ${persona.displayName}.`);
      setPersonaToDelete(null);
    } catch (error) {
      setPersonaErrorMessage(
        error instanceof Error ? error.message : "Failed to delete persona.",
      );
    }
  }

  async function handleSetPersonaActive(
    persona: AgentPersona,
    active: boolean,
    surface: PersonaFeedbackSurface,
  ) {
    clearPersonaFeedback(surface);

    try {
      await setPersonaActiveMutation.mutateAsync({
        id: persona.id,
        active,
      });
      setPersonaNoticeMessage(
        active
          ? `Selected ${persona.displayName} for My Agents.`
          : `Deselected ${persona.displayName} from My Agents.`,
      );
    } catch (error) {
      setPersonaErrorMessage(
        error instanceof Error
          ? error.message
          : active
            ? "Failed to select persona for My Agents."
            : "Failed to deselect persona from My Agents.",
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

  async function handlePersonaImportFile(
    fileBytes: number[],
    fileName: string,
  ) {
    clearPersonaFeedback("library");
    try {
      const result = await parsePersonaFiles(fileBytes, fileName);
      if (isSingleItemFile(fileBytes) && result.personas.length === 1) {
        setPersonaDialogState(importPersonaDialogState(result.personas[0]));
      } else if (result.personas.length > 0) {
        setBatchImportResult(result);
        setBatchImportFileName(fileName);
      } else {
        setPersonaErrorMessage("No valid personas found in file.");
      }
    } catch (err) {
      setPersonaErrorMessage(
        err instanceof Error ? err.message : "Failed to parse persona file.",
      );
    }
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
    setPersonaActiveMutation.isPending ||
    exportPersonaJsonMutation.isPending ||
    teamActions.exportTeamJsonMutation.isPending ||
    teamActions.createTeamMutation.isPending ||
    teamActions.updateTeamMutation.isPending ||
    teamActions.deleteTeamMutation.isPending;

  return (
    <>
      <div className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-4 sm:px-6">
        <div className="mx-auto flex w-full max-w-6xl flex-col gap-6">
          <div className="flex flex-col gap-6">
            <PersonasSection
              canChooseCatalog={catalogPersonas.length > 0}
              error={
                personasQuery.error instanceof Error
                  ? personasQuery.error
                  : null
              }
              feedbackErrorMessage={
                personaFeedbackSurface === "library"
                  ? personaErrorMessage
                  : null
              }
              feedbackNoticeMessage={
                personaFeedbackSurface === "library"
                  ? personaNoticeMessage
                  : null
              }
              isLoading={personasQuery.isLoading}
              isPending={
                createPersonaMutation.isPending ||
                updatePersonaMutation.isPending ||
                deletePersonaMutation.isPending ||
                setPersonaActiveMutation.isPending ||
                exportPersonaJsonMutation.isPending
              }
              onChooseCatalog={() => {
                clearPersonaFeedback("catalog");
                setIsCatalogDialogOpen(true);
              }}
              onCreate={() => {
                clearPersonaFeedback("library");
                setPersonaDialogState(createPersonaDialogState());
              }}
              onDelete={(persona) => {
                clearPersonaFeedback("library");
                setPersonaToDelete(persona);
              }}
              onDeactivate={(persona) => {
                void handleSetPersonaActive(persona, false, "library");
              }}
              onDuplicate={(persona) => {
                clearPersonaFeedback("library");
                setPersonaDialogState(duplicatePersonaDialogState(persona));
              }}
              onEdit={(persona) => {
                clearPersonaFeedback("library");
                setPersonaDialogState(editPersonaDialogState(persona));
              }}
              onImportFile={(fileBytes, fileName) => {
                void handlePersonaImportFile(fileBytes, fileName);
              }}
              onExport={(persona) => {
                clearPersonaFeedback("library");
                exportPersonaJsonMutation.mutate(persona.id, {
                  onSuccess: (saved) => {
                    if (saved) {
                      setPersonaNoticeMessage(
                        `Exported ${persona.displayName}.`,
                      );
                    }
                  },
                  onError: (error) => {
                    setPersonaErrorMessage(
                      error instanceof Error
                        ? error.message
                        : "Failed to export persona.",
                    );
                  },
                });
              }}
              personas={libraryPersonas}
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
              onExport={teamActions.handleExportTeam}
              onImportFile={teamActions.handleImportFile}
              onAddToChannel={teamActions.setTeamToAddToChannel}
              personas={libraryPersonas}
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
              logContent={managedAgentLogQuery.data?.content ?? null}
              logError={
                managedAgentLogQuery.error instanceof Error
                  ? managedAgentLogQuery.error
                  : null
              }
              logLoading={managedAgentLogQuery.isLoading}
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
              onSelectLogAgent={setLogAgentPubkey}
              onStart={(pubkey) => {
                void handleStart(pubkey);
              }}
              onStop={(pubkey) => {
                void handleStop(pubkey);
              }}
              onToggleStartOnAppLaunch={(pubkey, startOnAppLaunch) => {
                void handleToggleStartOnAppLaunch(pubkey, startOnAppLaunch);
              }}
              selectedLogAgentPubkey={logAgentPubkey}
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
        providers={acpProvidersQuery.data ?? []}
        providersLoading={acpProvidersQuery.isLoading}
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
      <PersonaCatalogDialog
        error={
          personasQuery.error instanceof Error ? personasQuery.error : null
        }
        feedbackErrorMessage={
          personaFeedbackSurface === "catalog" ? personaErrorMessage : null
        }
        feedbackNoticeMessage={
          personaFeedbackSurface === "catalog" ? personaNoticeMessage : null
        }
        isLoading={personasQuery.isLoading}
        isPending={setPersonaActiveMutation.isPending}
        onClearFeedback={() => {
          clearPersonaFeedback("catalog");
        }}
        onOpenChange={setIsCatalogDialogOpen}
        onSelectPersona={(persona, active) => {
          void handleSetPersonaActive(persona, active, "catalog");
        }}
        open={isCatalogDialogOpen}
        personas={catalogPersonas}
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
        personas={libraryPersonas}
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
        personas={libraryPersonas}
        team={teamActions.teamToAddToChannel}
      />
      <BatchImportDialog
        fileName={batchImportFileName}
        onComplete={(count) => {
          clearPersonaFeedback("library");
          setBatchImportResult(null);
          setPersonaNoticeMessage(
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
      <TeamImportDialog
        fileName={teamActions.teamImportPreview?.fileName ?? ""}
        onComplete={teamActions.handleTeamImportComplete}
        onOpenChange={(open) => {
          if (!open) {
            teamActions.setTeamImportPreview(null);
          }
        }}
        open={teamActions.teamImportPreview !== null}
        preview={teamActions.teamImportPreview?.preview ?? null}
      />
    </>
  );
}
