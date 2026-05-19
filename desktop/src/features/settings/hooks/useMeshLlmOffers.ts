import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { relayClient } from "@/shared/api/relayClient";
import type { RelayEvent } from "@/shared/api/types";

/**
 * Mesh-LLM offer envelope as carried in the `content` field of a kind:31990
 * event. Keep in sync with the Rust `sprout_core::mesh_llm::MeshLlmOffer`.
 */
export interface MeshLlmOffer {
  v: number;
  d_tag: string;
  endpoint_id: string;
  iroh_relay_url: string;
  /** Unix-seconds deadline; consumers ignore offers where `expires_at <= now`. */
  expires_at: number;
  caps: {
    max_vram_mb?: number | null;
    max_ram_mb?: number | null;
    max_concurrency?: number | null;
  };
  models: Array<{
    id: string;
    label?: string | null;
    context_tokens?: number | null;
  }>;
  extra?: unknown;
}

/**
 * A kind:31990 offer paired with the *Nostr* pubkey that signed it (so the
 * UI can show 'Alice is offering Llama 3 8B') and the event's `created_at`
 * (for sorting and freshness display).
 */
export interface ResolvedOffer {
  offer: MeshLlmOffer;
  pubkey: string;
  createdAt: number;
  d_tag: string;
}

function extractDTag(event: RelayEvent): string | null {
  for (const tag of event.tags) {
    if (tag.length >= 2 && tag[0] === "d") return tag[1];
  }
  return null;
}

/**
 * Canonicalise a relay URL the same way `sprout_core::mesh_llm`'s same-relay
 * filter does, so the JS side can't accept an offer the Rust schema check
 * would reject. (See `MeshLlmOffer::matches_local_relay` in core.)
 */
function canonicalRelayUrl(raw: string): string {
  const trimmed = raw.trim();
  const noQuery = trimmed.split("?")[0] ?? trimmed;
  const noFrag = noQuery.split("#")[0] ?? noQuery;
  const noTrail = noFrag.endsWith("/") ? noFrag.slice(0, -1) : noFrag;
  const protoIdx = noTrail.indexOf("://");
  if (protoIdx === -1) return noTrail;
  const scheme = noTrail.slice(0, protoIdx).toLowerCase();
  const rest = noTrail.slice(protoIdx + 3);
  const slash = rest.indexOf("/");
  if (slash === -1) {
    return `${scheme}://${rest.toLowerCase()}`;
  }
  const authority = rest.slice(0, slash).toLowerCase();
  const path = rest.slice(slash);
  return `${scheme}://${authority}${path}`;
}

/**
 * How often (in ms) the hook recomputes the rendered list so freshly
 * expired offers drop without waiting for a new event to arrive. Tied to a
 * setInterval rather than per-offer setTimeouts so the cost is O(1) no
 * matter how many offers are in the cache.
 */
const EXPIRY_TICK_MS = 30_000;

/**
 * Subscribe to live mesh-LLM offers from the connected relay.
 *
 * Returns the de-duplicated set of *currently-active* offers (keyed by
 * `(pubkey, d_tag)` per NIP-33), filtered to:
 * - the current relay's iroh-relay URL (v1 invariant: one relay = one mesh
 *   boundary). Offers advertising a different `iroh_relay_url` are dropped.
 * - non-expired offers (`expires_at > now`). Crashed publishers can't send
 *   the NIP-33 delete-by-replace tombstone; the TTL is the reaper.
 *
 * An event with empty `content` is treated as 'offer withdrawn' and removes
 * the corresponding entry — this is the NIP-33 delete-by-replace idiom the
 * Rust publisher emits when the user toggles compute-sharing off.
 */
export function useMeshLlmOffers(): {
  offers: ResolvedOffer[];
  error: string | null;
} {
  const [offers, setOffers] = useState<Map<string, ResolvedOffer>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const [localRelay, setLocalRelay] = useState<string | null>(null);
  const [nowSec, setNowSec] = useState(() => Math.floor(Date.now() / 1000));

  // Discover the current relay's iroh_relay_url once so we can filter
  // offers against it. If the relay doesn't advertise one, the same-relay
  // filter rejects everything and the panel correctly shows the empty
  // state.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const ws = await invoke<string>("get_relay_ws_url");
        const irohUrl = await invoke<string | null>("mesh_relay_iroh_url", {
          relayWsUrl: ws,
        });
        if (!cancelled) setLocalRelay(irohUrl ?? null);
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Periodic re-tick so expired offers fall out of the rendered list even
  // when no new events arrive.
  useEffect(() => {
    const id = setInterval(() => {
      setNowSec(Math.floor(Date.now() / 1000));
    }, EXPIRY_TICK_MS);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unsub: (() => Promise<void>) | null = null;

    function onEvent(event: RelayEvent) {
      if (cancelled) return;
      const dTag = extractDTag(event);
      if (!dTag) return;
      const key = `${event.pubkey}:${dTag}`;

      // Empty content = NIP-33 delete-by-replace.
      if (event.content.trim() === "") {
        setOffers((prev) => {
          if (!prev.has(key)) return prev;
          const next = new Map(prev);
          next.delete(key);
          return next;
        });
        return;
      }

      let parsed: MeshLlmOffer;
      try {
        parsed = JSON.parse(event.content) as MeshLlmOffer;
      } catch {
        // Skip malformed offers silently; one bad publisher must not
        // poison the list.
        return;
      }
      // Reject obviously-bad schema versions before storing.
      if (parsed.v !== 1) return;

      setOffers((prev) => {
        const existing = prev.get(key);
        if (existing && existing.createdAt >= event.created_at) {
          // We already have a fresher version under the same address.
          return prev;
        }
        const next = new Map(prev);
        next.set(key, {
          offer: parsed,
          pubkey: event.pubkey,
          createdAt: event.created_at,
          d_tag: dTag,
        });
        return next;
      });
    }

    (async () => {
      try {
        const u = await relayClient.subscribeToMeshLlmOffers(onEvent);
        if (cancelled) {
          void u();
        } else {
          unsub = u;
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();

    return () => {
      cancelled = true;
      if (unsub) void unsub();
    };
  }, []);

  // Filter on every render so newly-expired offers drop without a refresh
  // event, and so the same-relay filter applies as soon as the NIP-11
  // probe completes.
  const localCanonical =
    localRelay != null ? canonicalRelayUrl(localRelay) : null;
  const list = Array.from(offers.values())
    .filter((entry) => entry.offer.expires_at > nowSec)
    .filter((entry) => {
      if (localCanonical == null) return false;
      return canonicalRelayUrl(entry.offer.iroh_relay_url) === localCanonical;
    })
    .sort((a, b) => b.createdAt - a.createdAt);

  return { offers: list, error };
}
