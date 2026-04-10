import * as React from "react";

import type { PersonaCreatorOutput } from "@/features/agents/persona-creator";
import { toCreateInputs } from "@/features/agents/persona-creator";
import { personaCreatorChat } from "@/features/agents/persona-creator/chat";
import type { useCreatePersonaMutation } from "@/features/agents/hooks";

import type { useTeamActions } from "./useTeamActions";

/**
 * Encapsulates the AI persona creator chat logic:
 * sending messages, confirming creation, and tracking state.
 */
export function usePersonaChat(
  createPersonaMutation: ReturnType<typeof useCreatePersonaMutation>,
  teamActions: ReturnType<typeof useTeamActions>,
  setPersonaNoticeMessage: (msg: string) => void,
) {
  const [isOpen, setIsOpen] = React.useState(false);

  async function handleConfirmCreate(output: PersonaCreatorOutput) {
    const { personas: personaInputs, team: teamStub } = toCreateInputs(output);

    const createdIds: string[] = [];
    for (const input of personaInputs) {
      const created = await createPersonaMutation.mutateAsync(input);
      createdIds.push(created.id);
    }

    if (teamStub) {
      const personaIds = teamStub.personaIndices.map((idx) => createdIds[idx]);
      await teamActions.createTeamMutation.mutateAsync({
        name: teamStub.name,
        description: teamStub.description,
        personaIds,
      });
    }

    const count = createdIds.length;
    const teamNote = teamStub ? " and 1 team" : "";
    setPersonaNoticeMessage(
      `Created ${count} persona${count !== 1 ? "s" : ""}${teamNote} with AI.`,
    );
  }

  return {
    isOpen,
    setIsOpen,
    sendMessage: personaCreatorChat,
    handleConfirmCreate,
  };
}
