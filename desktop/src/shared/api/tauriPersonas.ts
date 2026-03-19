import { invokeTauri } from "@/shared/api/tauri";
import type {
  AgentPersona,
  CreatePersonaInput,
  UpdatePersonaInput,
} from "@/shared/api/types";

type RawPersona = {
  id: string;
  display_name: string;
  avatar_url: string | null;
  system_prompt: string;
  is_builtin: boolean;
  created_at: string;
  updated_at: string;
};

function fromRawPersona(persona: RawPersona): AgentPersona {
  return {
    id: persona.id,
    displayName: persona.display_name,
    avatarUrl: persona.avatar_url,
    systemPrompt: persona.system_prompt,
    isBuiltIn: persona.is_builtin,
    createdAt: persona.created_at,
    updatedAt: persona.updated_at,
  };
}

export async function listPersonas(): Promise<AgentPersona[]> {
  return (await invokeTauri<RawPersona[]>("list_personas")).map(fromRawPersona);
}

export async function createPersona(
  input: CreatePersonaInput,
): Promise<AgentPersona> {
  return fromRawPersona(
    await invokeTauri<RawPersona>("create_persona", {
      input: {
        displayName: input.displayName,
        avatarUrl: input.avatarUrl,
        systemPrompt: input.systemPrompt,
      },
    }),
  );
}

export async function updatePersona(
  input: UpdatePersonaInput,
): Promise<AgentPersona> {
  return fromRawPersona(
    await invokeTauri<RawPersona>("update_persona", {
      input: {
        id: input.id,
        displayName: input.displayName,
        avatarUrl: input.avatarUrl,
        systemPrompt: input.systemPrompt,
      },
    }),
  );
}

export async function deletePersona(id: string): Promise<void> {
  await invokeTauri("delete_persona", { id });
}
