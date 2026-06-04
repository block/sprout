import type { FeatureTier } from "./types";

/**
 * Pure resolution logic for feature visibility.
 * No side effects, no imports beyond types — safe to test in isolation.
 *
 * @param isDev - Whether the current build is a dev build.
 *               Defaults to `import.meta.env?.DEV ?? false` for runtime use.
 */
export function resolveEnabled(
  tier: FeatureTier,
  featureId: string,
  overrides: Record<string, boolean>,
  devToggle: boolean,
  isDev: boolean = (import.meta.env?.DEV as boolean) ?? false,
): boolean {
  switch (tier) {
    case "stable":
      return true;
    case "experimental":
      return overrides[featureId] === true;
    case "dev":
      if (!isDev) return false;
      if (!devToggle) return false;
      // Allow per-feature suppression even in dev
      return overrides[featureId] !== false;
    default:
      return false;
  }
}
