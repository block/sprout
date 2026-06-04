import { useSyncExternalStore, useCallback } from "react";
import { getFeature } from "./manifest";
import { getOverrides, getDevToggle, setOverride, setDevToggle } from "./store";
import type { FeatureTier } from "./types";

// ---------------------------------------------------------------------------
// Reactive store — components re-render when overrides change
// ---------------------------------------------------------------------------

type Listener = () => void;
const listeners = new Set<Listener>();

function subscribe(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function emitChange(): void {
  for (const listener of listeners) listener();
}

// Snapshot: a combined key of overrides + dev toggle for change detection
function getSnapshot(): string {
  return JSON.stringify({ o: getOverrides(), d: getDevToggle() });
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Returns whether a feature is enabled given its tier and user overrides.
 *
 * - stable: always true
 * - experimental: true only if user opted in
 * - dev: true only if in dev build AND global dev toggle is on
 */
export function useFeatureEnabled(featureId: string): boolean {
  const snapshot = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  const parsed = JSON.parse(snapshot) as {
    o: Record<string, boolean>;
    d: boolean;
  };

  const feature = getFeature(featureId);
  if (!feature) return false;

  return resolveEnabled(feature.tier, featureId, parsed.o, parsed.d);
}

/**
 * Hook to toggle a feature override. Returns [enabled, toggle].
 */
export function useFeatureToggle(
  featureId: string,
): [boolean, (enabled: boolean) => void] {
  const enabled = useFeatureEnabled(featureId);

  const toggle = useCallback(
    (value: boolean) => {
      setOverride(featureId, value);
      emitChange();
    },
    [featureId],
  );

  return [enabled, toggle];
}

/**
 * Hook for the global dev toggle. Returns [enabled, toggle].
 */
export function useDevToggle(): [boolean, (enabled: boolean) => void] {
  const snapshot = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  const parsed = JSON.parse(snapshot) as { d: boolean };

  const toggle = useCallback((value: boolean) => {
    setDevToggle(value);
    emitChange();
  }, []);

  return [parsed.d, toggle];
}

// ---------------------------------------------------------------------------
// Pure resolution logic (exported for testing)
// ---------------------------------------------------------------------------

export function resolveEnabled(
  tier: FeatureTier,
  featureId: string,
  overrides: Record<string, boolean>,
  devToggle: boolean,
): boolean {
  switch (tier) {
    case "stable":
      return true;
    case "experimental":
      return overrides[featureId] === true;
    case "dev":
      if (!import.meta.env.DEV) return false;
      if (!devToggle) return false;
      // Allow per-feature suppression even in dev
      return overrides[featureId] !== false;
    default:
      return false;
  }
}
