import { useSyncExternalStore, useCallback } from "react";
import { getFeature } from "./manifest";
import { resolveEnabled } from "./resolveEnabled";
import { getOverrides, getDevToggle, setOverride, setDevToggle } from "./store";

// ---------------------------------------------------------------------------
// Reactive store — components re-render when overrides change
// ---------------------------------------------------------------------------

type Listener = () => void;
const listeners = new Set<Listener>();

function subscribe(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Notify all subscribers that feature state changed */
export function emitChange(): void {
  // Invalidate cached snapshot
  cachedRaw = null;
  cachedParsed = null;
  for (const listener of listeners) listener();
}

// ---------------------------------------------------------------------------
// Cached snapshot — avoids JSON.parse on every render per hook instance
// ---------------------------------------------------------------------------

interface ParsedSnapshot {
  o: Record<string, boolean>;
  d: boolean;
}

let cachedRaw: string | null = null;
let cachedParsed: ParsedSnapshot | null = null;

function getSnapshot(): string {
  const raw = JSON.stringify({ o: getOverrides(), d: getDevToggle() });
  if (raw !== cachedRaw) {
    cachedRaw = raw;
    cachedParsed = JSON.parse(raw) as ParsedSnapshot;
  }
  return raw;
}

function getParsedSnapshot(): ParsedSnapshot {
  // Ensure snapshot is fresh
  getSnapshot();
  return cachedParsed!;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Returns the current parsed feature state (overrides + dev toggle).
 * Reactive — re-renders when any feature toggle changes.
 * Use this in components that need the full state (e.g. SettingsView filtering).
 */
export function useFeatureSnapshot(): ParsedSnapshot {
  useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  return getParsedSnapshot();
}

/**
 * Returns whether a feature is enabled given its tier and user overrides.
 *
 * - stable: always true
 * - experimental: true only if user opted in
 * - dev: true only if in dev build AND global dev toggle is on
 */
export function useFeatureEnabled(featureId: string): boolean {
  const snapshot = useFeatureSnapshot();

  const feature = getFeature(featureId);
  if (!feature) {
    if (import.meta.env.DEV) {
      console.warn(
        `[FeatureFlags] Unknown feature id: "${featureId}". Check features.json.`,
      );
    }
    return false;
  }

  return resolveEnabled(feature.tier, featureId, snapshot.o, snapshot.d);
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
  const snapshot = useFeatureSnapshot();

  const toggle = useCallback((value: boolean) => {
    setDevToggle(value);
    emitChange();
  }, []);

  return [snapshot.d, toggle];
}

// Re-export for consumers that imported from here
export { resolveEnabled } from "./resolveEnabled";
