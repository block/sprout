import * as React from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { relayClient } from "@/shared/api/relayClient";
import { getPresence, setPresence } from "@/shared/api/tauri";
import { normalizePubkey } from "@/shared/lib/pubkey";
import type { PresenceLookup, PresenceStatus } from "@/shared/api/types";
import {
  hydratePresence,
  registerWatcher,
  updatePresence as storeUpdatePresence,
  getWatchedPubkeys,
  getUnhydratedPubkeys,
  getGeneration,
  setOnNewWatchers,
  setOnReset,
  usePresenceStore,
  startTtlExpiry,
  chunkPubkeys,
} from "@/features/presence/presenceStore";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const PRESENCE_HEARTBEAT_INTERVAL_MS = 60_000;
const PRESENCE_IDLE_TIMEOUT_MS = 5 * 60_000;
const PRESENCE_STATUS_TICK_INTERVAL_MS = 30_000;
const PRESENCE_TTL_SECONDS = 90;
const PRESENCE_PREFERENCE_STORAGE_KEY = "sprout-presence-preference";
const HYDRATION_DEBOUNCE_MS = 100;

// ---------------------------------------------------------------------------
// Shared helpers (unchanged from original)
// ---------------------------------------------------------------------------

type PresencePreference = "auto" | "away" | "offline" | null;

function normalizePubkeys(pubkeys: string[]) {
  return [...new Set(pubkeys.map((pubkey) => normalizePubkey(pubkey)))]
    .filter((pubkey) => pubkey.length > 0)
    .sort();
}

function presencePreferenceStorageKey(pubkey: string) {
  return `${PRESENCE_PREFERENCE_STORAGE_KEY}:${pubkey}`;
}

function readStoredPresencePreference(pubkey: string): PresencePreference {
  if (typeof window === "undefined" || pubkey.length === 0) {
    return null;
  }

  const value = window.localStorage.getItem(
    presencePreferenceStorageKey(pubkey),
  );
  return value === "auto" || value === "away" || value === "offline"
    ? value
    : null;
}

function writeStoredPresencePreference(
  pubkey: string,
  preference: PresencePreference,
) {
  if (typeof window === "undefined" || pubkey.length === 0) {
    return;
  }

  if (preference === null) {
    window.localStorage.removeItem(presencePreferenceStorageKey(pubkey));
    return;
  }

  window.localStorage.setItem(presencePreferenceStorageKey(pubkey), preference);
}

function resolveAutomaticPresenceStatus(
  isDocumentHidden: boolean,
  lastActivityAt: number,
  now: number,
): PresenceStatus {
  if (isDocumentHidden) {
    return "away";
  }

  return now - lastActivityAt >= PRESENCE_IDLE_TIMEOUT_MS ? "away" : "online";
}

// ---------------------------------------------------------------------------
// Hydration helper — fetch in chunks of 200, merge into store.
// Generation-fenced: discards results if workspace switched during flight.
// Per-pubkey de-duplicated: overlapping callers await the same in-flight
// Promise for shared pubkeys, preventing redundant REST calls.
// ---------------------------------------------------------------------------

/** Per-pubkey in-flight Promise. Cleared on workspace reset. */
let inflightPromises = new Map<string, Promise<void>>();

/** Clear in-flight tracking (called on workspace reset via store callback). */
function clearInflight(): void {
  inflightPromises = new Map();
}

/**
 * Fetch presence for the given pubkeys. For any pubkey already in-flight,
 * awaits the existing Promise instead of issuing a new request.
 *
 * Race-safe: registers toFetch pubkeys BEFORE awaiting existing promises,
 * so concurrent callers see them as in-flight immediately. Cleans up only
 * entries that still point to this batch's promise (avoids clobbering a
 * newer request's entry).
 */
async function fetchAndHydrate(pubkeys: string[]): Promise<void> {
  if (pubkeys.length === 0) {
    return;
  }

  const gen = getGeneration();
  const requestStartedAt = Date.now();

  // Split into: pubkeys we need to fetch vs. pubkeys already in-flight
  const toFetch: string[] = [];
  const toAwait: Promise<void>[] = [];

  for (const pk of pubkeys) {
    const existing = inflightPromises.get(pk);
    if (existing) {
      toAwait.push(existing);
    } else {
      toFetch.push(pk);
    }
  }

  // If we have pubkeys to fetch, create the batch promise and register
  // them BEFORE awaiting existing promises (prevents race window).
  let batchPromise: Promise<void> | null = null;

  if (toFetch.length > 0) {
    batchPromise = (async () => {
      const chunks = chunkPubkeys(toFetch);
      const results = await Promise.all(
        chunks.map((chunk) => getPresence(chunk)),
      );

      // Discard if workspace switched while we were fetching
      if (getGeneration() !== gen) {
        return;
      }

      const merged: PresenceLookup = Object.assign({}, ...results);
      hydratePresence(merged, requestStartedAt);
    })();

    // Register in-flight for each pubkey immediately
    for (const pk of toFetch) {
      inflightPromises.set(pk, batchPromise);
    }
  }

  // Await all work: existing in-flight + our new batch (if any).
  // Use allSettled so one failure doesn't prevent others from completing.
  const allWork = [...toAwait];
  if (batchPromise) {
    allWork.push(batchPromise);
  }

  const results = await Promise.allSettled(allWork);

  // Clean up in-flight entries — only remove if they still point to our batch
  if (batchPromise && getGeneration() === gen) {
    for (const pk of toFetch) {
      if (inflightPromises.get(pk) === batchPromise) {
        inflightPromises.delete(pk);
      }
    }
  }

  // Propagate ANY rejection relevant to this caller's pubkeys — not just
  // our own batch. If an awaited in-flight request failed, the caller
  // should know those pubkeys weren't hydrated.
  const firstRejection = results.find((r) => r.status === "rejected");
  if (firstRejection && firstRejection.status === "rejected") {
    throw firstRejection.reason;
  }
}

// ---------------------------------------------------------------------------
// usePresenceQuery — same external signature, reads from store
// ---------------------------------------------------------------------------

export function usePresenceQuery(
  pubkeys: string[],
  options?: { enabled?: boolean },
): {
  data: PresenceLookup;
  isLoading: boolean;
  isSuccess: boolean;
  error: Error | null;
} {
  const normalizedPubkeys = normalizePubkeys(pubkeys);
  const enabled = (options?.enabled ?? true) && normalizedPubkeys.length > 0;

  // Stable key for memoization
  const pubkeysKey = normalizedPubkeys.join(",");

  const [isLoading, setIsLoading] = React.useState(enabled);
  const [isSuccess, setIsSuccess] = React.useState(false);
  const [error, setError] = React.useState<Error | null>(null);

  // Register watchers and trigger hydration on mount / pubkey change.
  // Uses a local `cancelled` flag to prevent stale async results from
  // updating state after pubkeys change or the component unmounts.
  // biome-ignore lint/correctness/useExhaustiveDependencies: normalizedPubkeys is derived from pubkeysKey — listing pubkeysKey as the dep is sufficient and avoids array identity churn
  React.useEffect(() => {
    if (!enabled) {
      setIsLoading(false);
      setIsSuccess(false);
      setError(null);
      return;
    }

    let cancelled = false;
    // skipUnhydrated: we hydrate directly below, so don't add to the
    // debounced queue (prevents duplicate fetches for the same pubkeys).
    const unregister = registerWatcher(normalizedPubkeys, {
      skipUnhydrated: true,
    });

    setIsLoading(true);
    setIsSuccess(false);
    setError(null);

    fetchAndHydrate(normalizedPubkeys)
      .then(() => {
        if (!cancelled) {
          setIsSuccess(true);
          setIsLoading(false);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setError(err instanceof Error ? err : new Error(String(err)));
          setIsLoading(false);
        }
      });

    return () => {
      cancelled = true;
      unregister();
    };
  }, [enabled, pubkeysKey]);

  const data = usePresenceStore(enabled ? normalizedPubkeys : []);

  // Reactive recovery: if we're in an error/loading state but the store now
  // has data for ALL our watched pubkeys (e.g., hydrateAll() succeeded later),
  // clear the error and mark success. Requires every pubkey to be resolved —
  // a single WS update for one pubkey won't prematurely clear the error state.
  const allResolved =
    enabled &&
    normalizedPubkeys.length > 0 &&
    normalizedPubkeys.every((pk) => pk in data);
  React.useEffect(() => {
    if (allResolved && (error !== null || (isLoading && !isSuccess))) {
      setError(null);
      setIsSuccess(true);
      setIsLoading(false);
    }
  }, [allResolved, error, isLoading, isSuccess]);

  return { data, isLoading, isSuccess, error };
}

// ---------------------------------------------------------------------------
// usePresenceSubscription — called ONCE in AppShell
// ---------------------------------------------------------------------------

export function usePresenceSubscription(): void {
  React.useEffect(() => {
    let isCancelled = false;
    let unsub: (() => Promise<void>) | null = null;
    let debounceTimer: ReturnType<typeof setTimeout> | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    // Start TTL expiry interval
    const stopTtl = startTtlExpiry();

    // Register reset callback to clear in-flight hydration tracking
    setOnReset(clearInflight);

    // Debounced hydration for newly-registered pubkeys.
    // Triggered by the store's onNewWatchers callback (event-driven, not polled).
    function scheduleHydration() {
      if (isCancelled) {
        return;
      }
      if (debounceTimer !== null) {
        clearTimeout(debounceTimer);
      }
      debounceTimer = setTimeout(() => {
        debounceTimer = null;
        if (isCancelled) {
          return;
        }
        const unhydrated = getUnhydratedPubkeys();
        if (unhydrated.length > 0) {
          void fetchAndHydrate(unhydrated).catch((err) => {
            console.error("[presence] debounced hydration failed:", err);
          });
        }
      }, HYDRATION_DEBOUNCE_MS);
    }

    // Register the callback so the store notifies us when new watchers appear
    setOnNewWatchers(scheduleHydration);

    // Full re-hydration of all watched pubkeys
    async function hydrateAll() {
      const watched = getWatchedPubkeys();
      if (watched.length === 0) {
        return;
      }
      try {
        await fetchAndHydrate(watched);
      } catch (err) {
        console.error("[usePresenceSubscription] hydration failed:", err);
      }
    }

    // Subscribe to kind:20001 presence events with retry on failure.
    // On first successful subscription, also trigger a full hydration
    // (covers relay-down-at-startup: individual hook fetches may have failed,
    // but once WS connects we know the relay is reachable).
    function subscribeWithRetry(attempt = 0) {
      if (isCancelled) {
        return;
      }
      void relayClient
        .subscribeToPresenceUpdates((event) => {
          if (isCancelled) {
            return;
          }
          const status = event.content;
          if (
            status === "online" ||
            status === "away" ||
            status === "offline"
          ) {
            storeUpdatePresence(event.pubkey, status);
          }
        })
        .then((unsubFn) => {
          if (isCancelled) {
            void unsubFn();
            return;
          }
          unsub = unsubFn;
          // On first successful subscription, hydrate all watched pubkeys.
          // This covers two cases:
          // 1. Relay was down at startup (attempt > 0) — individual hook fetches failed
          // 2. Relay was up but some hook fetches raced with subscription setup
          // The per-pubkey de-dupe ensures this doesn't duplicate work that's
          // already in-flight from individual usePresenceQuery mounts.
          void hydrateAll();
        })
        .catch(() => {
          // Retry with exponential backoff: 1s, 2s, 4s, 8s, max 30s
          if (!isCancelled) {
            const delay = Math.min(1000 * 2 ** attempt, 30_000);
            retryTimer = setTimeout(
              () => subscribeWithRetry(attempt + 1),
              delay,
            );
          }
        });
    }
    subscribeWithRetry();

    // On reconnect: re-hydrate all watched pubkeys
    const unsubReconnect = relayClient.subscribeToReconnects(() => {
      if (!isCancelled) {
        void hydrateAll();
      }
    });

    return () => {
      isCancelled = true;
      setOnNewWatchers(null);
      setOnReset(null);
      stopTtl();
      unsubReconnect();
      if (debounceTimer !== null) {
        clearTimeout(debounceTimer);
      }
      if (retryTimer !== null) {
        clearTimeout(retryTimer);
      }
      if (unsub) {
        void unsub();
      }
    };
  }, []);
}

// ---------------------------------------------------------------------------
// useSetPresenceMutation — keep existing + optimistic store update
// ---------------------------------------------------------------------------

export function useSetPresenceMutation(pubkey?: string) {
  const queryClient = useQueryClient();
  const normalizedPubkey = pubkey?.trim().toLowerCase() ?? "";

  return useMutation({
    mutationFn: async (status: PresenceStatus) => {
      const gen = getGeneration();
      let result: { status: PresenceStatus; ttlSeconds: number };

      try {
        result = await setPresence(status);
      } catch (error) {
        if (
          !(error instanceof Error) ||
          (!error.message.includes("relay returned 404") &&
            !error.message.includes("relay returned 405"))
        ) {
          throw error;
        }

        await relayClient.sendPresence(status);
        result = {
          status,
          ttlSeconds: status === "offline" ? 0 : PRESENCE_TTL_SECONDS,
        };
      }

      return { ...result, gen };
    },
    onSuccess: ({ status, gen }) => {
      if (normalizedPubkey.length === 0) {
        return;
      }

      // Discard if workspace switched during the mutation flight
      if (gen !== getGeneration()) {
        return;
      }

      // Optimistic local update — no need to wait for the next WS event
      storeUpdatePresence(normalizedPubkey, status);

      // Keep react-query cache in sync for any legacy consumers
      void queryClient.invalidateQueries({ queryKey: ["presence"] });
    },
  });
}

// ---------------------------------------------------------------------------
// usePresenceSession — heartbeat logic preserved exactly
// ---------------------------------------------------------------------------

export function usePresenceSession(pubkey?: string) {
  const normalizedPubkey = pubkey?.trim().toLowerCase() ?? "";
  const presenceQuery = usePresenceQuery(
    normalizedPubkey.length > 0 ? [normalizedPubkey] : [],
    { enabled: normalizedPubkey.length > 0 },
  );
  const setPresenceMutation = useSetPresenceMutation(normalizedPubkey);
  const [presencePreference, setPresencePreference] =
    React.useState<PresencePreference>(() =>
      readStoredPresencePreference(normalizedPubkey),
    );
  const [lastActivityAt, setLastActivityAt] = React.useState(() => Date.now());
  const [statusClock, setStatusClock] = React.useState(() => Date.now());
  const [isDocumentHidden, setIsDocumentHidden] = React.useState(() =>
    typeof document === "undefined" ? false : document.hidden,
  );
  const skipNextSyncRef = React.useRef<PresenceStatus | null>(null);

  React.useEffect(() => {
    const now = Date.now();
    setPresencePreference(readStoredPresencePreference(normalizedPubkey));
    setLastActivityAt(now);
    setStatusClock(now);
    setIsDocumentHidden(
      typeof document === "undefined" ? false : document.hidden,
    );
  }, [normalizedPubkey]);

  React.useEffect(() => {
    writeStoredPresencePreference(normalizedPubkey, presencePreference);
  }, [normalizedPubkey, presencePreference]);

  const recordActivity = React.useEffectEvent(() => {
    const now = Date.now();
    setLastActivityAt(now);
    setStatusClock(now);
  });

  React.useEffect(() => {
    if (normalizedPubkey.length === 0) {
      return;
    }

    function handleUserActivity() {
      if (typeof document !== "undefined" && document.hidden) {
        return;
      }

      recordActivity();
    }

    function handleFocus() {
      setIsDocumentHidden(false);
      recordActivity();
    }

    function handleVisibilityChange() {
      const hidden = document.hidden;
      setIsDocumentHidden(hidden);

      if (!hidden) {
        recordActivity();
      }
    }

    window.addEventListener("pointerdown", handleUserActivity, true);
    window.addEventListener("keydown", handleUserActivity, true);
    window.addEventListener("focus", handleFocus);
    document.addEventListener("visibilitychange", handleVisibilityChange);

    return () => {
      window.removeEventListener("pointerdown", handleUserActivity, true);
      window.removeEventListener("keydown", handleUserActivity, true);
      window.removeEventListener("focus", handleFocus);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [normalizedPubkey]);

  React.useEffect(() => {
    if (normalizedPubkey.length === 0) {
      return;
    }

    const intervalId = window.setInterval(() => {
      setStatusClock(Date.now());
    }, PRESENCE_STATUS_TICK_INTERVAL_MS);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [normalizedPubkey]);

  const automaticStatus = React.useMemo(
    () =>
      resolveAutomaticPresenceStatus(
        isDocumentHidden,
        lastActivityAt,
        statusClock,
      ),
    [isDocumentHidden, lastActivityAt, statusClock],
  );
  const currentStatus =
    normalizedPubkey.length === 0
      ? "offline"
      : presencePreference === "offline"
        ? "offline"
        : presencePreference === "away"
          ? "away"
          : presencePreference === "auto"
            ? automaticStatus
            : automaticStatus;

  const updatePresence = React.useCallback(
    async (status: PresenceStatus) => {
      const previousPreference = presencePreference;
      const nextPreference: PresencePreference =
        status === "online" ? "auto" : status;

      if (nextPreference === "auto") {
        const now = Date.now();
        setLastActivityAt(now);
        setStatusClock(now);
        setIsDocumentHidden(
          typeof document === "undefined" ? false : document.hidden,
        );
      }

      setPresencePreference(nextPreference);
      skipNextSyncRef.current = status;

      try {
        await setPresenceMutation.mutateAsync(status);
      } catch (error) {
        skipNextSyncRef.current = null;
        setPresencePreference(previousPreference);
        throw error;
      }
    },
    [presencePreference, setPresenceMutation],
  );

  const syncPresence = React.useEffectEvent((status: PresenceStatus) => {
    void setPresenceMutation.mutateAsync(status).catch(() => {
      return;
    });
  });

  React.useEffect(() => {
    if (normalizedPubkey.length === 0) {
      return;
    }

    if (skipNextSyncRef.current === currentStatus) {
      skipNextSyncRef.current = null;
      return;
    }

    syncPresence(currentStatus);
  }, [currentStatus, normalizedPubkey]);

  React.useEffect(() => {
    if (normalizedPubkey.length === 0 || currentStatus === "offline") {
      return;
    }

    const intervalId = window.setInterval(() => {
      syncPresence(currentStatus);
    }, PRESENCE_HEARTBEAT_INTERVAL_MS);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [currentStatus, normalizedPubkey]);

  return {
    currentStatus,
    isLoading: presenceQuery.isLoading,
    isPending: setPresenceMutation.isPending,
    error:
      setPresenceMutation.error instanceof Error
        ? setPresenceMutation.error
        : presenceQuery.error instanceof Error
          ? presenceQuery.error
          : null,
    setStatus: updatePresence,
  };
}
