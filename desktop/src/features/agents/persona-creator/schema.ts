import type { CreatePersonaInput, CreateTeamInput } from "@/shared/api/types";

// ---------------------------------------------------------------------------
// Types for the persona-creator agent's structured output
// ---------------------------------------------------------------------------

export type PersonaCreatorPersona = {
  displayName: string;
  systemPrompt: string;
  provider?: string;
  model?: string;
  namePool?: string[];
};

export type PersonaCreatorTeam = {
  name: string;
  description?: string;
  /** Indices into the personas array - mapped to real IDs after creation. */
  personaIndices: number[];
};

export type PersonaCreatorOutput = {
  personas: PersonaCreatorPersona[];
  team?: PersonaCreatorTeam;
};

// ---------------------------------------------------------------------------
// JSON Schema (embedded in the system prompt so the agent knows the contract)
// ---------------------------------------------------------------------------

export const personaCreatorJsonSchema = {
  type: "object",
  required: ["personas"],
  properties: {
    personas: {
      type: "array",
      minItems: 1,
      items: {
        type: "object",
        required: ["displayName", "systemPrompt"],
        properties: {
          displayName: { type: "string", description: "Human-readable name" },
          systemPrompt: {
            type: "string",
            description: "The persona's system prompt",
          },
          provider: {
            type: "string",
            description: "Optional ACP provider id (e.g. goose, claude)",
          },
          model: {
            type: "string",
            description: "Optional model id (e.g. claude-sonnet-4-20250514)",
          },
          namePool: {
            type: "array",
            items: { type: "string" },
            description:
              "Optional pool of display names the agent rotates through",
          },
        },
      },
    },
    team: {
      type: "object",
      required: ["name", "personaIndices"],
      properties: {
        name: { type: "string" },
        description: { type: "string" },
        personaIndices: {
          type: "array",
          items: { type: "integer", minimum: 0 },
          description:
            "Zero-based indices into the personas array for team membership",
        },
      },
    },
  },
} as const;

// ---------------------------------------------------------------------------
// Parsing utilities
// ---------------------------------------------------------------------------

/** Extract the first fenced JSON block from agent response text. */
export function extractJsonBlock(text: string): string | null {
  const fencePattern = /```(?:json)?\s*\n([\s\S]*?)\n```/;
  const match = fencePattern.exec(text);
  return match?.[1]?.trim() ?? null;
}

/** Validate raw JSON into a typed PersonaCreatorOutput. */
export function parsePersonaCreatorOutput(
  raw: unknown,
): { ok: true; data: PersonaCreatorOutput } | { ok: false; error: string } {
  if (typeof raw !== "object" || raw === null) {
    return { ok: false, error: "Expected a JSON object" };
  }

  const obj = raw as Record<string, unknown>;

  if (!Array.isArray(obj.personas) || obj.personas.length === 0) {
    return { ok: false, error: "personas must be a non-empty array" };
  }

  const personas: PersonaCreatorPersona[] = [];
  for (let i = 0; i < obj.personas.length; i++) {
    const p = obj.personas[i] as Record<string, unknown>;
    if (typeof p.displayName !== "string" || !p.displayName.trim()) {
      return { ok: false, error: `personas[${i}].displayName is required` };
    }
    if (typeof p.systemPrompt !== "string" || !p.systemPrompt.trim()) {
      return { ok: false, error: `personas[${i}].systemPrompt is required` };
    }
    personas.push({
      displayName: p.displayName,
      systemPrompt: p.systemPrompt,
      provider: typeof p.provider === "string" ? p.provider : undefined,
      model: typeof p.model === "string" ? p.model : undefined,
      namePool: Array.isArray(p.namePool)
        ? p.namePool.filter((n): n is string => typeof n === "string")
        : undefined,
    });
  }

  let team: PersonaCreatorTeam | undefined;
  if (obj.team != null) {
    const t = obj.team as Record<string, unknown>;
    if (typeof t.name !== "string" || !t.name.trim()) {
      return { ok: false, error: "team.name is required" };
    }
    if (!Array.isArray(t.personaIndices)) {
      return { ok: false, error: "team.personaIndices must be an array" };
    }
    const indices: number[] = [];
    for (let j = 0; j < t.personaIndices.length; j++) {
      const idx = t.personaIndices[j];
      if (typeof idx !== "number" || idx < 0 || idx >= personas.length) {
        return {
          ok: false,
          error: `team.personaIndices[${j}] is out of range (${idx} >= ${personas.length})`,
        };
      }
      indices.push(idx);
    }
    team = {
      name: t.name,
      description:
        typeof t.description === "string" ? t.description : undefined,
      personaIndices: indices,
    };
  }

  return { ok: true, data: { personas, team } };
}

/**
 * Convert validated output to CreatePersonaInput[] and a team stub.
 * The caller creates personas first, collects their IDs, then maps indices
 * to IDs for the createTeam call.
 */
export function toCreateInputs(output: PersonaCreatorOutput): {
  personas: CreatePersonaInput[];
  team:
    | (Omit<CreateTeamInput, "personaIds"> & { personaIndices: number[] })
    | null;
} {
  const personas: CreatePersonaInput[] = output.personas.map((p) => ({
    displayName: p.displayName,
    systemPrompt: p.systemPrompt,
    provider: p.provider,
    model: p.model,
    namePool: p.namePool,
  }));

  const team = output.team
    ? {
        name: output.team.name,
        description: output.team.description,
        personaIndices: output.team.personaIndices,
      }
    : null;

  return { personas, team };
}
