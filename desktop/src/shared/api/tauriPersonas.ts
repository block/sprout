import { invokeTauri } from "@/shared/api/tauri";
import type {
  AgentPersona,
  CreatePersonaInput,
  UpdatePersonaInput,
} from "@/shared/api/types";

// Raw types matching Rust snake_case output
type RawParsedPersonaPreview = {
  display_name: string;
  system_prompt: string;
  avatar_data_url: string | null;
  source_file: string;
};

type RawSkippedFile = {
  source_file: string;
  reason: string;
};

type RawParsePersonaFilesResult = {
  personas: RawParsedPersonaPreview[];
  skipped: RawSkippedFile[];
};

// Public camelCase types
export type ParsedPersonaPreview = {
  displayName: string;
  systemPrompt: string;
  avatarDataUrl: string | null;
  sourceFile: string;
};

export type SkippedFile = {
  sourceFile: string;
  reason: string;
};

export type ParsePersonaFilesResult = {
  personas: ParsedPersonaPreview[];
  skipped: SkippedFile[];
};

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

export async function parsePersonaFiles(
  fileBytes: number[],
  fileName: string,
): Promise<ParsePersonaFilesResult> {
  const raw = await invokeTauri<RawParsePersonaFilesResult>(
    "parse_persona_files",
    { fileBytes, fileName },
  );
  return {
    personas: raw.personas.map((p) => ({
      displayName: p.display_name,
      systemPrompt: p.system_prompt,
      avatarDataUrl: p.avatar_data_url,
      sourceFile: p.source_file,
    })),
    skipped: raw.skipped.map((s) => ({
      sourceFile: s.source_file,
      reason: s.reason,
    })),
  };
}

export async function exportPersonaToPng(id: string): Promise<boolean> {
  return invokeTauri<boolean>("export_persona_to_png", { id });
}
