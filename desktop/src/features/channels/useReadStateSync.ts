import * as React from "react";

import { relayClient } from "@/shared/api/relayClient";
import { nip44EncryptToSelf, signRelayEvent } from "@/shared/api/tauri";
import type { RelayEvent } from "@/shared/api/types";
import { KIND_READ_STATE } from "@/shared/constants/kinds";

import {
  CLIENT_ID_STORAGE_KEY,
  SLOT_ID_STORAGE_KEY,
  type ReadStateBlob,
  type DecodedBlob,
  randomHex,
  getOrCreateStorageValue,
  readSyncEnabled,
  writeSyncEnabled,
  readCachedReadState,
  writeCachedReadState,
  validateDTag,
  decryptAndValidateBlob,
  mergeContexts,
  contextsEqual,
} from "@/features/channels/lib/readStateSync";

// ── The hook ─────────────────────────────────────────────────────────────────

export function useReadStateSync(userPubkey: string | undefined) {
  // Merged effective read state: contextId → unix seconds
  const [mergedState, setMergedState] =
    React.useState<Record<string, number>>(readCachedReadState);
  const [syncEnabled, setSyncEnabledState] = React.useState(readSyncEnabled);

  // Refs for the sync engine (mutable, not triggering re-renders)
  const clientIdRef = React.useRef<string>("");
  const slotIdRef = React.useRef<string>("");
  const ownBlobRef = React.useRef<Record<string, number>>({});
  const lastPublishedRef = React.useRef<Record<string, number>>({});
  const maxFetchedCreatedAtRef = React.useRef<number>(0);
  const debounceTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const isInitializedRef = React.useRef(false);
  const isPublishingRef = React.useRef(false);
  const needsRepublishRef = React.useRef(false);
  const republishTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );

  // Update cache whenever merged state changes
  React.useEffect(() => {
    writeCachedReadState(mergedState);
  }, [mergedState]);

  // Settings toggle
  const setSyncEnabled = React.useCallback((enabled: boolean) => {
    writeSyncEnabled(enabled);
    setSyncEnabledState(enabled);
  }, []);

  // ── Publish own blob ───────────────────────────────────────────────────

  const publishOwnBlob = React.useCallback(
    async (retryDepth = 0) => {
      if (!userPubkey || !syncEnabled || !isInitializedRef.current) return;

      // Publish serialization: skip if already in-flight, flag for retry after
      if (isPublishingRef.current) {
        needsRepublishRef.current = true;
        return;
      }
      isPublishingRef.current = true;

      try {
        // Read-before-write: fetch own blob by slot-ID (NIP-RS §Read-before-write)
        const slotId = slotIdRef.current;

        const events = await relayClient.fetchOwnReadStateBlob(
          userPubkey,
          slotId,
        );

        // Validate and merge fetched blob(s)
        let fetchedOwnContexts: Record<string, number> = {};
        let maxCreatedAt = maxFetchedCreatedAtRef.current;

        for (const event of events) {
          maxCreatedAt = Math.max(maxCreatedAt, event.created_at);

          const decoded = await decryptAndValidateBlob(event);
          if (!decoded) continue;

          // Slot-ID conflict check
          if (decoded.blob.client_id !== clientIdRef.current) {
            // Another client has claimed our slot — regenerate
            const newSlotId = randomHex(32);
            localStorage.setItem(SLOT_ID_STORAGE_KEY, newSlotId);
            slotIdRef.current = newSlotId;
            // Retry with new slot, up to 3 attempts
            if (retryDepth >= 3) {
              console.error(
                "[ReadStateSync] Slot-ID conflict retry limit reached (3 attempts)",
              );
              return;
            }
            isPublishingRef.current = false;
            return publishOwnBlob(retryDepth + 1);
          }

          fetchedOwnContexts = mergeContexts(
            fetchedOwnContexts,
            decoded.blob.contexts,
          );
        }

        maxFetchedCreatedAtRef.current = maxCreatedAt;

        // Merge fetched with local
        const merged = mergeContexts(fetchedOwnContexts, ownBlobRef.current);
        ownBlobRef.current = merged;

        // Suppress publish if unchanged
        if (contextsEqual(merged, lastPublishedRef.current)) return;

        // Build the blob payload
        const payload: ReadStateBlob = {
          v: 1,
          client_id: clientIdRef.current,
          contexts: merged,
        };

        const encrypted = await nip44EncryptToSelf(JSON.stringify(payload));

        // NIP-RS §Clock Skew: created_at MUST be max(now, max_fetched + 1)
        const now = Math.floor(Date.now() / 1000);
        const createdAt = Math.max(now, maxFetchedCreatedAtRef.current + 1);

        const event = await signRelayEvent({
          kind: KIND_READ_STATE,
          content: encrypted,
          tags: [
            ["d", `read-state:${slotIdRef.current}`],
            ["t", "read-state"],
          ],
          created_at: createdAt,
        });

        await relayClient.publishReadStateEvent(event);
        lastPublishedRef.current = { ...merged };
      } catch (error) {
        console.error("[ReadStateSync] Failed to publish read state:", error);
      } finally {
        isPublishingRef.current = false;
        // If a publish was requested while we were in-flight, retry now
        if (needsRepublishRef.current) {
          needsRepublishRef.current = false;
          void publishOwnBlob();
        }
      }
    },
    [userPubkey, syncEnabled],
  );

  // ── Mark a channel as read ─────────────────────────────────────────────

  const markContextRead = React.useCallback(
    (contextId: string, timestamp: number) => {
      // Update local state immediately
      setMergedState((current) => {
        const existing = current[contextId] ?? 0;
        if (timestamp <= existing) return current;
        return { ...current, [contextId]: timestamp };
      });

      // Update own blob
      const existingOwn = ownBlobRef.current[contextId] ?? 0;
      if (timestamp > existingOwn) {
        ownBlobRef.current = {
          ...ownBlobRef.current,
          [contextId]: timestamp,
        };
      }

      // Schedule debounced publish
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }
      debounceTimerRef.current = setTimeout(() => {
        debounceTimerRef.current = null;
        void publishOwnBlob();
      }, 5_000);
    },
    [publishOwnBlob],
  );

  // ── Initialization: fetch + merge + subscribe ──────────────────────────

  React.useEffect(() => {
    if (!userPubkey) return;
    // Capture narrowed type for closures below
    const pubkey: string = userPubkey;

    const clientId = getOrCreateStorageValue(CLIENT_ID_STORAGE_KEY);
    const slotId = getOrCreateStorageValue(SLOT_ID_STORAGE_KEY);
    clientIdRef.current = clientId;
    slotIdRef.current = slotId;

    let disposed = false;
    let subscribeDispose: (() => Promise<void>) | undefined;

    async function initialize() {
      const since = Math.floor(Date.now() / 1000) - 7 * 86400;

      // Fetch all read-state blobs within horizon
      let events: RelayEvent[];
      try {
        events = await relayClient.fetchReadStateEvents(pubkey, since);
      } catch (error) {
        console.error("[ReadStateSync] Failed to fetch read state:", error);
        isInitializedRef.current = true;
        return;
      }

      if (disposed) return;

      // Decode all blobs
      const decoded: DecodedBlob[] = [];
      for (const event of events) {
        const result = await decryptAndValidateBlob(event);
        if (result) decoded.push(result);
      }

      if (disposed) return;

      // Find own blob(s)
      const ownBlobs = decoded.filter((d) => d.blob.client_id === clientId);

      // Handle duplicate client_id: use highest created_at
      if (ownBlobs.length > 1) {
        ownBlobs.sort((a, b) => b.event.created_at - a.event.created_at);
        // Delete stale duplicates via kind:5
        for (let i = 1; i < ownBlobs.length; i++) {
          const stale = ownBlobs[i];
          if (!stale) continue;
          const staleD = validateDTag(stale.event);
          if (staleD) {
            try {
              const deleteEvent = await signRelayEvent({
                kind: 5,
                content: "",
                tags: [["a", `${KIND_READ_STATE}:${pubkey}:${staleD}`]],
              });
              await relayClient.publishReadStateEvent(deleteEvent);
            } catch {
              // Best-effort deletion
            }
          }
        }
      }

      // Slot-ID conflict detection
      const ownDTag = `read-state:${slotId}`;
      const conflictingBlob = decoded.find((d) => {
        const dTag = validateDTag(d.event);
        return dTag === ownDTag && d.blob.client_id !== clientId;
      });
      if (conflictingBlob) {
        const newSlotId = randomHex(32);
        localStorage.setItem(SLOT_ID_STORAGE_KEY, newSlotId);
        slotIdRef.current = newSlotId;
      }

      // Track max created_at
      let maxCreatedAt = 0;
      for (const d of decoded) {
        maxCreatedAt = Math.max(maxCreatedAt, d.event.created_at);
      }
      maxFetchedCreatedAtRef.current = maxCreatedAt;

      // Set own blob from highest-priority own blob
      const primaryOwnBlob = ownBlobs[0];
      if (primaryOwnBlob) {
        ownBlobRef.current = { ...primaryOwnBlob.blob.contexts };
        lastPublishedRef.current = { ...primaryOwnBlob.blob.contexts };
      }

      // Merge ALL blobs (own + others) using max() per context
      let effective: Record<string, number> = {};
      for (const d of decoded) {
        effective = mergeContexts(effective, d.blob.contexts);
      }

      // Merge with cached localStorage state (may have local reads not yet published)
      const cached = readCachedReadState();
      effective = mergeContexts(effective, cached);

      // Also merge own blob ref (may have writes since cache was read)
      effective = mergeContexts(effective, ownBlobRef.current);

      if (!disposed) {
        // Use functional update to merge with any marks that arrived while
        // initialize() was running (e.g. from useUnreadChannels init effect).
        // A plain `setMergedState(effective)` would overwrite those marks.
        setMergedState((current) => mergeContexts(current, effective));
        isInitializedRef.current = true;
      }

      // ── Live subscription ────────────────────────────────────────────
      if (disposed) return;

      void relayClient
        .subscribeToReadState(pubkey, (event: RelayEvent) => {
          if (disposed) return;
          void handleIncomingEvent(event);
        })
        .then((dispose) => {
          if (disposed) {
            void dispose?.();
          } else {
            subscribeDispose = dispose;
          }
        })
        .catch((error) => {
          console.error(
            "[ReadStateSync] Failed to subscribe to read state:",
            error,
          );
        });
    }

    async function handleIncomingEvent(event: RelayEvent) {
      const decoded = await decryptAndValidateBlob(event);
      if (!decoded) return;

      // Update max created_at
      maxFetchedCreatedAtRef.current = Math.max(
        maxFetchedCreatedAtRef.current,
        event.created_at,
      );

      if (decoded.blob.client_id === clientIdRef.current) {
        // Our own blob echoed back — update own ref
        ownBlobRef.current = mergeContexts(
          ownBlobRef.current,
          decoded.blob.contexts,
        );
        setMergedState((current) =>
          mergeContexts(current, decoded.blob.contexts),
        );
        return;
      }

      // Another client's blob — merge into state
      let needsRepublish = false;

      setMergedState((current) => {
        const merged = mergeContexts(current, decoded.blob.contexts);

        // Check if any context advanced beyond our last-published state
        for (const [ctx, ts] of Object.entries(decoded.blob.contexts)) {
          if (ts > (lastPublishedRef.current[ctx] ?? 0)) {
            needsRepublish = true;
            break;
          }
        }

        return merged;
      });

      // Schedule re-publish if another device advanced a context beyond our
      // last publish — but do NOT absorb their data into ownBlobRef.
      if (needsRepublish && syncEnabled) {
        if (republishTimerRef.current) {
          clearTimeout(republishTimerRef.current);
        }
        republishTimerRef.current = setTimeout(() => {
          republishTimerRef.current = null;
          void publishOwnBlob();
        }, 5_000);
      }
    }

    void initialize();

    return () => {
      disposed = true;
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
        debounceTimerRef.current = null;
      }
      if (republishTimerRef.current) {
        clearTimeout(republishTimerRef.current);
        republishTimerRef.current = null;
      }
      // Handle fast-unmount: if subscribe resolved, dispose immediately;
      // otherwise the .then() handler will dispose when it resolves.
      if (subscribeDispose) {
        void subscribeDispose();
      }
      // Reset refs so a re-mount starts fresh
      isInitializedRef.current = false;
      isPublishingRef.current = false;
      needsRepublishRef.current = false;
      ownBlobRef.current = {};
      lastPublishedRef.current = {};
      maxFetchedCreatedAtRef.current = 0;
    };
  }, [userPubkey, publishOwnBlob, syncEnabled]);

  return {
    /** Merged effective read state: contextId → unix seconds (0 = unknown). */
    mergedState,
    /** Mark a context as read up to a unix timestamp (seconds). */
    markContextRead,
    /** Whether cross-device sync publishing is enabled. */
    syncEnabled,
    /** Toggle cross-device sync publishing. */
    setSyncEnabled,
  };
}
