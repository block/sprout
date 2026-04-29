/**
 * presenceStore — module-level singleton for presence state.
 *
 * Mental model:
 *   presenceMap   — what we know: pubkey → { status, updatedAt }
 *   watcherCounts — ref-counted: pubkey → # of hooks watching it
 *   unhydrated    — pubkeys added since last hydration batch
 *   generation    — monotonic counter; incremented on resetStore() to fence
 *                   in-flight async writes from a previous workspace
 *
 * React integration: useSyncExternalStore with per-hook selectors (no new deps).
 * TTL: any entry older than 95s is marked "offline" every 30s.
 * Hydration: chunked in batches of 200 (server cap).
 * Bounded: only stores entries for watched pubkeys.
 */

import { useCallback, useRef, useSyncExternalStore } from "react";

import type { PresenceLookup, PresenceStatus } from "@/shared/api/types";

const PRESENCE_TTL_MS = 95_000; // 90s server TTL + 5s grace
const HYDRATION_CHUNK_SIZE = 200;

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

type PresenceEntry = { status: PresenceStatus; updatedAt: number };

let presenceMap = new Map<string, PresenceEntry>();
let watcherCounts = new Map<string, number>();
let unhydratedPubkeys = new Set<string>();

/**
 * Monotonic generation counter. Incremented on resetStore().
 * All async operations capture the generation at start and discard results
 * if the generation has changed (workspace switched during flight).
 */
let generation = 0;

const listeners = new Set<() => void>();

// ---------------------------------------------------------------------------
// Notify
// ---------------------------------------------------------------------------

function notify(): void {
  for (const listener of listeners) {
    listener();
  }
}

// ---------------------------------------------------------------------------
// Public store API
// ---------------------------------------------------------------------------

/** Current generation — capture before async work, check after. */
export function getGeneration(): number {
  return generation;
}

/**
 * Update a single pubkey's presence status.
 * Only stores if the pubkey is currently watched (bounded cache).
 */
export function updatePresence(pubkey: string, status: PresenceStatus): void {
  // Only store if someone is watching this pubkey
  if (!watcherCounts.has(pubkey)) {
    return;
  }

  const existing = presenceMap.get(pubkey);
  if (existing?.status === status) {
    // Refresh updatedAt even if status unchanged (resets TTL clock)
    existing.updatedAt = Date.now();
    return;
  }
  presenceMap.set(pubkey, { status, updatedAt: Date.now() });
  notify();
}

/**
 * Bulk-seed presence from a hydration response.
 * Only writes entries for watched pubkeys. Skips entries whose updatedAt
 * is newer than the hydration timestamp (prevents overwriting fresh WS data).
 */
export function hydratePresence(
  lookup: PresenceLookup,
  requestStartedAt: number,
): void {
  let changed = false;
  const now = Date.now();
  for (const [pubkey, status] of Object.entries(lookup)) {
    // Only store watched pubkeys
    if (!watcherCounts.has(pubkey)) {
      continue;
    }
    const existing = presenceMap.get(pubkey);
    // Don't overwrite if we received a fresher WS event after the request started
    if (existing && existing.updatedAt > requestStartedAt) {
      continue;
    }
    if (!existing || existing.status !== status) {
      presenceMap.set(pubkey, { status, updatedAt: now });
      changed = true;
    } else {
      existing.updatedAt = now;
    }
  }
  if (changed) {
    notify();
  }
}

/** Callback invoked when new pubkeys need hydration. Set by usePresenceSubscription. */
let onNewWatchersCallback: (() => void) | null = null;

/** Register the callback for new-watcher notifications. */
export function setOnNewWatchers(cb: (() => void) | null): void {
  onNewWatchersCallback = cb;
}

/**
 * Register watchers for a set of pubkeys.
 * Ref-counts: 0→1 marks pubkey as needing hydration.
 * Pass `skipUnhydrated: true` if the caller will hydrate directly (avoids
 * double-fetch from the debounced path).
 * Returns an unregister function (call on unmount).
 */
export function registerWatcher(
  pubkeys: string[],
  options?: { skipUnhydrated?: boolean },
): () => void {
  let hasNew = false;
  for (const pubkey of pubkeys) {
    const count = watcherCounts.get(pubkey) ?? 0;
    watcherCounts.set(pubkey, count + 1);
    if (count === 0 && !options?.skipUnhydrated) {
      // First watcher — needs hydration unless caller handles it
      unhydratedPubkeys.add(pubkey);
      hasNew = true;
    }
  }

  // Notify the subscription hook that new pubkeys need hydration
  if (hasNew && onNewWatchersCallback) {
    onNewWatchersCallback();
  }

  return () => {
    for (const pubkey of pubkeys) {
      const count = watcherCounts.get(pubkey) ?? 0;
      if (count <= 1) {
        watcherCounts.delete(pubkey);
        // Prune the entry — nobody's watching, free the memory
        presenceMap.delete(pubkey);
      } else {
        watcherCounts.set(pubkey, count - 1);
      }
    }
  };
}

/** All pubkeys currently watched (refcount > 0). */
export function getWatchedPubkeys(): string[] {
  return Array.from(watcherCounts.keys());
}

/**
 * Pubkeys added since the last hydration batch.
 * Calling this drains the set (marks them as "hydration in-flight").
 */
export function getUnhydratedPubkeys(): string[] {
  const result = Array.from(unhydratedPubkeys);
  unhydratedPubkeys.clear();
  return result;
}

/**
 * Split an array into chunks of at most `size`.
 * Used for chunked hydration against the 200-pubkey server cap.
 */
export function chunkPubkeys(
  pubkeys: string[],
  size: number = HYDRATION_CHUNK_SIZE,
): string[][] {
  const chunks: string[][] = [];
  for (let i = 0; i < pubkeys.length; i += size) {
    chunks.push(pubkeys.slice(i, i + size));
  }
  return chunks;
}

/** Callback to clear external in-flight state on reset. Set by hooks module. */
let onResetCallback: (() => void) | null = null;

/** Register a callback invoked during resetStore (for clearing in-flight maps). */
export function setOnReset(cb: (() => void) | null): void {
  onResetCallback = cb;
}

/**
 * Reset all store state. Called on workspace switch so stale presence
 * from the previous workspace never bleeds into the new one.
 * Increments generation to invalidate any in-flight async operations.
 */
export function resetStore(): void {
  generation += 1;
  presenceMap = new Map();
  watcherCounts = new Map();
  unhydratedPubkeys = new Set();
  if (onResetCallback) {
    onResetCallback();
  }
  notify();
}

// ---------------------------------------------------------------------------
// TTL expiry — runs every 30s, marks entries older than 95s as "offline"
// ---------------------------------------------------------------------------

function runTtlExpiry(): void {
  const cutoff = Date.now() - PRESENCE_TTL_MS;
  let changed = false;
  for (const [pubkey, entry] of presenceMap) {
    if (entry.status !== "offline" && entry.updatedAt < cutoff) {
      presenceMap.set(pubkey, {
        status: "offline",
        updatedAt: entry.updatedAt,
      });
      changed = true;
    }
  }
  if (changed) {
    notify();
  }
}

let ttlIntervalId: ReturnType<typeof setInterval> | null = null;

/** Start the TTL expiry interval. Returns a stop function. */
export function startTtlExpiry(): () => void {
  if (ttlIntervalId !== null) {
    return () => {};
  }
  ttlIntervalId = setInterval(runTtlExpiry, 30_000);
  return () => {
    if (ttlIntervalId !== null) {
      clearInterval(ttlIntervalId);
      ttlIntervalId = null;
    }
  };
}

// ---------------------------------------------------------------------------
// useSyncExternalStore wiring — selector-based to avoid global re-renders
// ---------------------------------------------------------------------------

function storeSubscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

// ---------------------------------------------------------------------------
// React hook
// ---------------------------------------------------------------------------

/**
 * Subscribe to presence for a set of pubkeys.
 * Returns a stable PresenceLookup filtered to the requested pubkeys.
 *
 * Uses useSyncExternalStore with a per-hook snapshot that only changes
 * when the specific watched pubkeys' statuses change — prevents global
 * re-renders when unrelated pubkeys update.
 */
export function usePresenceStore(pubkeys: string[]): PresenceLookup {
  const prevRef = useRef<PresenceLookup>({});

  // biome-ignore lint/correctness/useExhaustiveDependencies: pubkeys is captured by value via join(",") — the callback is stable when the pubkey set is stable
  const getSlice = useCallback((): PresenceLookup => {
    const prev = prevRef.current;
    let same = true;

    // Quick check: did any watched pubkey's status change?
    for (const pubkey of pubkeys) {
      const entry = presenceMap.get(pubkey);
      const currentStatus = entry?.status;
      const prevStatus = prev[pubkey];
      if (currentStatus !== prevStatus) {
        same = false;
        break;
      }
    }
    // Also check if prev had extra keys (pubkeys list shrank)
    if (
      same &&
      Object.keys(prev).length !==
        pubkeys.filter((k) => presenceMap.has(k)).length
    ) {
      same = false;
    }

    if (same) {
      return prev;
    }

    const next: PresenceLookup = {};
    for (const pubkey of pubkeys) {
      const entry = presenceMap.get(pubkey);
      if (entry) {
        next[pubkey] = entry.status;
      }
    }
    prevRef.current = next;
    return next;
  }, [pubkeys.join(",")]);

  return useSyncExternalStore(storeSubscribe, getSlice, getSlice);
}
