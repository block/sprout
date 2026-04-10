import type { ParsePersonaFilesResult } from "@/shared/api/tauriPersonas";
import type {
  AgentPersona,
  CreatePersonaInput,
  UpdatePersonaInput,
} from "@/shared/api/types";

export type PersonaDialogState = {
  description: string;
  initialValues: CreatePersonaInput | UpdatePersonaInput;
  submitLabel: string;
  title: string;
};

type ParsedPersonaDraft = ParsePersonaFilesResult["personas"][number];

export function createPersonaDialogState(): PersonaDialogState {
  return {
    title: "Create persona",
    description:
      "Save a reusable role, prompt, and optional avatar for future agent deployments.",
    submitLabel: "Create persona",
    initialValues: {
      displayName: "",
      avatarUrl: "",
      systemPrompt: "",
      provider: undefined,
      model: undefined,
    },
  };
}

export function duplicatePersonaDialogState(
  persona: AgentPersona,
): PersonaDialogState {
  return {
    title: `Duplicate ${persona.displayName}`,
    description:
      "Create a new persona by copying this template and adjusting it as needed.",
    submitLabel: "Create persona",
    initialValues: {
      displayName: `${persona.displayName} copy`,
      avatarUrl: persona.avatarUrl ?? "",
      systemPrompt: persona.systemPrompt,
      provider: persona.provider ?? undefined,
      model: persona.model ?? undefined,
    },
  };
}

export function editPersonaDialogState(
  persona: AgentPersona,
): PersonaDialogState {
  return {
    title: "Edit persona",
    description: "",
    submitLabel: "Save changes",
    initialValues: {
      id: persona.id,
      displayName: persona.displayName,
      avatarUrl: persona.avatarUrl ?? "",
      systemPrompt: persona.systemPrompt,
      provider: persona.provider ?? undefined,
      model: persona.model ?? undefined,
    },
  };
}

export function importPersonaDialogState(
  persona: ParsedPersonaDraft,
): PersonaDialogState {
  return {
    title: `Import ${persona.displayName}`,
    description: "Review and save this imported persona.",
    submitLabel: "Create persona",
    initialValues: {
      displayName: persona.displayName,
      avatarUrl: persona.avatarDataUrl ?? "",
      systemPrompt: persona.systemPrompt,
      provider: persona.provider ?? undefined,
      model: persona.model ?? undefined,
    },
  };
}
