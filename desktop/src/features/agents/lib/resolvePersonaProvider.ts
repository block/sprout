import type { AcpProvider } from "@/shared/api/types";

/**
 * Result of resolving a persona's preferred provider against the set of
 * currently-available ACP providers.
 *
 * `provider` is the provider that should be used for deployment.
 * `warnings` contains user-visible messages when the resolved provider
 * differs from what the persona requested (e.g. the configured runtime
 * was uninstalled) or when no provider is available at all.
 */
export type ResolvePersonaProviderResult = {
  provider: AcpProvider | null;
  warnings: string[];
};

/**
 * Resolve which ACP provider to use when deploying an agent from a persona.
 *
 * Resolution order:
 * 1. If the persona has no `providerId` → use `defaultProvider`, no warnings.
 * 2. If the persona's `providerId` matches an available provider → use it.
 * 3. If the persona's `providerId` is set but not found in `providers` →
 *    fall back to `defaultProvider` and emit a warning.
 * 4. If there is no `defaultProvider` either → return `null` with an error
 *    warning so the UI can block deployment.
 */
export function resolvePersonaProvider(
  personaProviderId: string | undefined | null,
  providers: readonly AcpProvider[],
  defaultProvider: AcpProvider | null,
): ResolvePersonaProviderResult {
  // Case 1: Persona has no provider preference — use the default.
  if (!personaProviderId) {
    return {
      provider: defaultProvider,
      warnings: defaultProvider
        ? []
        : [
            "No agent runtimes are available. Install a runtime (e.g. Goose) to deploy agents.",
          ],
    };
  }

  // Case 2: Persona's preferred provider is available.
  const matched = providers.find((p) => p.id === personaProviderId);
  if (matched) {
    return { provider: matched, warnings: [] };
  }

  // Case 3 & 4: Persona's provider is not available — fall back.
  if (defaultProvider) {
    return {
      provider: defaultProvider,
      warnings: [
        `Persona is configured for runtime "${personaProviderId}" but it is not available. Using ${defaultProvider.label} instead.`,
      ],
    };
  }

  return {
    provider: null,
    warnings: [
      `Persona is configured for runtime "${personaProviderId}" but it is not available, and no other runtimes were found.`,
    ],
  };
}

/**
 * Collect provider-resolution warnings for a list of personas.
 *
 * Used by deploy dialogs to surface inline alerts when one or more
 * personas reference a runtime that isn't currently available.
 */
export function collectProviderWarnings(
  personas: readonly { provider: string | null }[],
  providers: readonly AcpProvider[],
  fallbackProvider: AcpProvider | null,
): string[] {
  if (!fallbackProvider) return [];
  const warnings: string[] = [];
  for (const persona of personas) {
    const { warnings: w } = resolvePersonaProvider(
      persona.provider,
      providers,
      fallbackProvider,
    );
    warnings.push(...w);
  }
  return warnings;
}
