import * as React from "react";

import { relayClient } from "@/shared/api/relayClient";
import type { RelaySubscriptionFilter } from "@/shared/api/relayClientShared";

type SubscriptionFilter = Omit<RelaySubscriptionFilter, "limit" | "since">;

/**
 * Subscribe to live relay events matching `filter` and invalidate React Query
 * cache via `onInvalidate` whenever a new event arrives or the WebSocket
 * reconnects.
 *
 * Pass `null` as the filter to skip subscribing (e.g. when a required ID isn't
 * available yet). The reconnect listener is still registered so cache is
 * refreshed after connection recovery even when the subscription is skipped.
 *
 * @param filter       Nostr subscription filter, or `null` to disable.
 * @param onInvalidate Called on each incoming event and on reconnect.
 * @param label        Human-readable label for error logging (e.g. "forum").
 */
export function useReactiveSubscription(
  filter: SubscriptionFilter | null,
  onInvalidate: () => void,
  label: string,
) {
  // Stabilise the invalidation callback so the effect doesn't re-run when
  // the caller passes an inline arrow.
  const onInvalidateRef = React.useRef(onInvalidate);
  React.useLayoutEffect(() => {
    onInvalidateRef.current = onInvalidate;
  });

  // Memoize the filter by its serialized content so callers can pass object
  // literals without churning the subscription effect.
  const filterKey = filter === null ? "null" : stableFilterKey(filter);
  // biome-ignore lint/correctness/useExhaustiveDependencies: filterKey is the stable serialization of filter
  const stableFilter = React.useMemo(() => filter, [filterKey]);

  React.useEffect(() => {
    let isCancelled = false;
    let cleanup: (() => Promise<void>) | undefined;

    const invalidate = () => {
      onInvalidateRef.current();
    };

    const disposeReconnect = relayClient.subscribeToReconnects(invalidate);

    if (stableFilter === null) {
      return () => {
        disposeReconnect();
      };
    }

    relayClient
      .subscribeLive(
        { ...stableFilter, limit: 0, since: Math.floor(Date.now() / 1_000) },
        () => {
          if (!isCancelled) {
            invalidate();
          }
        },
      )
      .then((dispose) => {
        if (isCancelled) {
          void dispose();
          return;
        }
        cleanup = dispose;
      })
      .catch((error) => {
        console.error(`Failed to subscribe to ${label} events`, error);
      });

    return () => {
      isCancelled = true;
      disposeReconnect();
      if (cleanup) void cleanup();
    };
  }, [label, stableFilter]);
}

/**
 * Produce a stable string key for a filter object so the effect only re-runs
 * when the filter semantically changes.
 */
function stableFilterKey(filter: SubscriptionFilter): string {
  return JSON.stringify(filter, Object.keys(filter).sort());
}
