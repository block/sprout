/**
 * Persistence layer for feature flag overrides.
 *
 * localStorage keys:
 *   sprout-feature-overrides  — JSON object of { [featureId]: boolean }
 *   sprout-dev-features       — "true" | "false" (global dev toggle)
 */

const OVERRIDES_KEY = "sprout-feature-overrides";
const DEV_TOGGLE_KEY = "sprout-dev-features";

export type FeatureOverrides = Record<string, boolean>;

/** Read all user overrides from localStorage */
export function getOverrides(): FeatureOverrides {
  try {
    const raw = window.localStorage.getItem(OVERRIDES_KEY);
    return raw ? (JSON.parse(raw) as FeatureOverrides) : {};
  } catch {
    return {};
  }
}

/** Persist a single feature override */
export function setOverride(featureId: string, enabled: boolean): void {
  const overrides = getOverrides();
  overrides[featureId] = enabled;
  window.localStorage.setItem(OVERRIDES_KEY, JSON.stringify(overrides));
}

/** Remove a single feature override (revert to default) */
export function clearOverride(featureId: string): void {
  const overrides = getOverrides();
  delete overrides[featureId];
  window.localStorage.setItem(OVERRIDES_KEY, JSON.stringify(overrides));
}

/** Whether the global "Show developer features" toggle is on */
export function getDevToggle(): boolean {
  try {
    const raw = window.localStorage.getItem(DEV_TOGGLE_KEY);
    // Default to true in dev builds, false in prod
    if (raw === null) return import.meta.env.DEV;
    return raw === "true";
  } catch {
    return import.meta.env.DEV;
  }
}

/** Set the global dev toggle */
export function setDevToggle(enabled: boolean): void {
  window.localStorage.setItem(DEV_TOGGLE_KEY, enabled ? "true" : "false");
}
