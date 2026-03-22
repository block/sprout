import * as React from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import {
  personasQueryKey,
  teamsQueryKey,
  useCreateTeamMutation,
  useDeleteTeamMutation,
  useTeamsQuery,
  useUpdateTeamMutation,
} from "@/features/agents/hooks";
import type { CreateChannelManagedAgentsResult } from "@/features/agents/channelAgents";
import {
  type ParsedTeamPreview,
  createTeam as createTeamApi,
  exportTeamToJson,
  parseTeamFile,
} from "@/shared/api/tauriTeams";
import type {
  AgentTeam,
  Channel,
  CreateTeamInput,
  UpdateTeamInput,
} from "@/shared/api/types";

type TeamDialogState = {
  description: string;
  initialValues: CreateTeamInput | UpdateTeamInput;
  submitLabel: string;
  title: string;
} | null;

type ActionMessages = {
  setActionNoticeMessage: (message: string | null) => void;
  setActionErrorMessage: (message: string | null) => void;
};

type RefetchCallbacks = {
  refetchManagedAgents: () => void;
  refetchRelayAgents: () => void;
};

export function useTeamActions(
  actions: ActionMessages,
  refetch: RefetchCallbacks,
) {
  const queryClient = useQueryClient();
  const teamsQuery = useTeamsQuery();
  const createTeamMutation = useCreateTeamMutation();
  const updateTeamMutation = useUpdateTeamMutation();
  const deleteTeamMutation = useDeleteTeamMutation();

  const exportTeamJsonMutation = useMutation({
    mutationFn: (id: string) => exportTeamToJson(id),
  });

  const [teamDialogState, setTeamDialogState] =
    React.useState<TeamDialogState>(null);
  const [teamToDelete, setTeamToDelete] = React.useState<AgentTeam | null>(
    null,
  );
  const [teamToAddToChannel, setTeamToAddToChannel] =
    React.useState<AgentTeam | null>(null);
  const [teamImportPreview, setTeamImportPreview] = React.useState<{
    preview: ParsedTeamPreview;
    fileName: string;
  } | null>(null);

  const teams = teamsQuery.data ?? [];

  async function handleTeamSubmit(input: CreateTeamInput | UpdateTeamInput) {
    actions.setActionNoticeMessage(null);
    actions.setActionErrorMessage(null);

    try {
      if ("id" in input) {
        await updateTeamMutation.mutateAsync(input);
        actions.setActionNoticeMessage(`Updated team "${input.name}".`);
      } else {
        await createTeamMutation.mutateAsync(input);
        actions.setActionNoticeMessage(`Created team "${input.name}".`);
      }
      setTeamDialogState(null);
    } catch (error) {
      actions.setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to save team.",
      );
    }
  }

  async function handleDeleteTeam(team: AgentTeam) {
    actions.setActionNoticeMessage(null);
    actions.setActionErrorMessage(null);

    try {
      await deleteTeamMutation.mutateAsync(team.id);
      actions.setActionNoticeMessage(`Deleted team "${team.name}".`);
      setTeamToDelete(null);
    } catch (error) {
      actions.setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to delete team.",
      );
    }
  }

  function handleTeamDeployed(
    channel: Channel,
    result: CreateChannelManagedAgentsResult,
  ) {
    actions.setActionErrorMessage(null);
    const successCount = result.successes.length;
    const failCount = result.failures.length;
    if (failCount === 0) {
      actions.setActionNoticeMessage(
        `Deployed ${successCount} ${successCount === 1 ? "agent" : "agents"} to ${channel.name}.`,
      );
    } else {
      actions.setActionNoticeMessage(
        `Deployed ${successCount} ${successCount === 1 ? "agent" : "agents"} to ${channel.name}. ${failCount} failed.`,
      );
    }
    setTeamToAddToChannel(null);
    refetch.refetchManagedAgents();
    refetch.refetchRelayAgents();
  }

  function openCreateDialog() {
    actions.setActionNoticeMessage(null);
    actions.setActionErrorMessage(null);
    setTeamDialogState({
      title: "Create team",
      description: "Group personas together for quick deployment to channels.",
      submitLabel: "Create team",
      initialValues: {
        name: "",
        description: "",
        personaIds: [],
      },
    });
  }

  function openDuplicateDialog(team: AgentTeam) {
    actions.setActionNoticeMessage(null);
    actions.setActionErrorMessage(null);
    setTeamDialogState({
      title: `Duplicate ${team.name}`,
      description: "Create a new team by copying this one.",
      submitLabel: "Create team",
      initialValues: {
        name: `${team.name} copy`,
        description: team.description ?? "",
        personaIds: [...team.personaIds],
      },
    });
  }

  function handleExportTeam(team: AgentTeam) {
    exportTeamJsonMutation.mutate(team.id, {
      onSuccess: (saved) => {
        if (saved) {
          actions.setActionNoticeMessage(`Exported team "${team.name}".`);
        }
      },
      onError: (err) => {
        actions.setActionErrorMessage(
          err instanceof Error ? err.message : "Failed to export team.",
        );
      },
    });
  }

  function handleImportFile(fileBytes: number[], fileName: string) {
    actions.setActionNoticeMessage(null);
    actions.setActionErrorMessage(null);
    void (async () => {
      try {
        const preview = await parseTeamFile(fileBytes, fileName);
        setTeamImportPreview({ preview, fileName });
      } catch (err) {
        actions.setActionErrorMessage(
          err instanceof Error ? err.message : "Failed to parse team file.",
        );
      }
    })();
  }

  function handleTeamImportComplete(
    teamName: string,
    teamDescription: string | null,
    personaIds: string[],
  ) {
    setTeamImportPreview(null);
    void (async () => {
      const teamInput = {
        name: teamName,
        description: teamDescription ?? undefined,
        personaIds,
      };

      // Try creating the team, retry once on failure.
      for (let attempt = 0; attempt < 2; attempt++) {
        try {
          await createTeamApi(teamInput);
          actions.setActionNoticeMessage(
            `Imported team "${teamName}" with ${personaIds.length} persona${personaIds.length !== 1 ? "s" : ""}.`,
          );
          void queryClient.invalidateQueries({ queryKey: personasQueryKey });
          void queryClient.invalidateQueries({ queryKey: teamsQueryKey });
          return;
        } catch {
          if (attempt === 0) continue;
        }
      }

      // Both attempts failed — personas exist but team doesn't.
      actions.setActionErrorMessage(
        `Imported ${personaIds.length} persona${personaIds.length !== 1 ? "s" : ""} but failed to create team "${teamName}". The personas are saved — create a team manually to group them.`,
      );
      void queryClient.invalidateQueries({ queryKey: personasQueryKey });
    })();
  }

  function openEditDialog(team: AgentTeam) {
    actions.setActionNoticeMessage(null);
    actions.setActionErrorMessage(null);
    setTeamDialogState({
      title: `Edit ${team.name}`,
      description: "Update this team's name, description, or personas.",
      submitLabel: "Save changes",
      initialValues: {
        id: team.id,
        name: team.name,
        description: team.description ?? "",
        personaIds: [...team.personaIds],
      },
    });
  }

  return {
    teams,
    teamsQuery,
    createTeamMutation,
    updateTeamMutation,
    deleteTeamMutation,
    exportTeamJsonMutation,
    teamDialogState,
    setTeamDialogState,
    teamToDelete,
    setTeamToDelete,
    teamToAddToChannel,
    setTeamToAddToChannel,
    teamImportPreview,
    setTeamImportPreview,
    handleTeamSubmit,
    handleDeleteTeam,
    handleTeamDeployed,
    handleExportTeam,
    handleImportFile,
    handleTeamImportComplete,
    openCreateDialog,
    openDuplicateDialog,
    openEditDialog,
  };
}
