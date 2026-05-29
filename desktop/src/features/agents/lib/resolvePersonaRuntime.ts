import type { AcpRuntime } from "@/shared/api/types";

/**
 * Result of resolving a persona's preferred runtime against the set of
 * currently-available ACP runtimes.
 *
 * `runtime` is the runtime that should be used for deployment.
 * `warnings` contains user-visible messages when the resolved runtime
 * differs from what the persona requested (e.g. the configured runtime
 * was uninstalled) or when no runtime is available at all.
 */
export type ResolvePersonaRuntimeResult = {
  runtime: AcpRuntime | null;
  warnings: string[];
};

/**
 * Resolve which ACP runtime to use when deploying an agent from a persona.
 *
 * Resolution order:
 * 1. If the persona has no `runtimeId` → use `defaultRuntime`, no warnings.
 * 2. If the persona's `runtimeId` matches an available runtime → use it.
 * 3. If the persona's `runtimeId` is set but not found in `runtimes` →
 *    fall back to `defaultRuntime` and emit a warning.
 * 4. If there is no `defaultRuntime` either → return `null` with an error
 *    warning so the UI can block deployment.
 */
export function resolvePersonaRuntime(
  personaRuntimeId: string | undefined | null,
  runtimes: readonly AcpRuntime[],
  defaultRuntime: AcpRuntime | null,
): ResolvePersonaRuntimeResult {
  // Case 1: Persona has no runtime preference — use the default.
  if (!personaRuntimeId) {
    return {
      runtime: defaultRuntime,
      warnings: defaultRuntime
        ? []
        : [
            "No agent runtimes are available. Install a runtime (e.g. Goose) to deploy agents.",
          ],
    };
  }

  // Case 2: Persona's preferred runtime is available.
  const matched = runtimes.find((p) => p.id === personaRuntimeId);
  if (matched) {
    return { runtime: matched, warnings: [] };
  }

  // Case 3 & 4: Persona's runtime is not available — fall back.
  if (defaultRuntime) {
    return {
      runtime: defaultRuntime,
      warnings: [
        `Persona is configured for runtime "${personaRuntimeId}" but it is not available. Using ${defaultRuntime.label} instead.`,
      ],
    };
  }

  return {
    runtime: null,
    warnings: [
      `Persona is configured for runtime "${personaRuntimeId}" but it is not available, and no other runtimes were found.`,
    ],
  };
}

/**
 * Collect runtime-resolution warnings for a list of personas.
 *
 * Used by deploy dialogs to surface inline alerts when one or more
 * personas reference a runtime that isn't currently available.
 */
export function collectRuntimeWarnings(
  personas: readonly { runtime: string | null }[],
  runtimes: readonly AcpRuntime[],
  fallbackRuntime: AcpRuntime | null,
): string[] {
  if (!fallbackRuntime) return [];
  const warnings: string[] = [];
  for (const persona of personas) {
    const { warnings: w } = resolvePersonaRuntime(
      persona.runtime,
      runtimes,
      fallbackRuntime,
    );
    warnings.push(...w);
  }
  return warnings;
}
