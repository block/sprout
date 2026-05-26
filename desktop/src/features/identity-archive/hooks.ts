import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { useIdentityQuery } from "@/shared/api/hooks";
import {
  archiveIdentity,
  listArchivedIdentities,
  resolveOaOwner,
  unarchiveIdentity,
  type ArchivedIdentitiesSnapshot,
  type IdentityArchiveRequest,
  type IdentityUnarchiveRequest,
} from "@/shared/api/tauriIdentityArchive";

export const archivedIdentitiesQueryKey = ["archivedIdentities"] as const;

/** Cache the relay's `kind:13535` snapshot. Drives the "Archived" flair. */
export function useArchivedIdentitiesQuery(enabled = true) {
  return useQuery<ArchivedIdentitiesSnapshot>({
    enabled,
    queryKey: archivedIdentitiesQueryKey,
    queryFn: listArchivedIdentities,
    staleTime: 30_000,
  });
}

/**
 * `true` iff `pubkey` appears in the relay's latest archive snapshot.
 * Returns `undefined` while the snapshot is loading so callers can hide the
 * flair until we know.
 */
export function useIsIdentityArchived(pubkey: string): boolean | undefined {
  const query = useArchivedIdentitiesQuery();
  if (!query.data) return undefined;
  const lower = pubkey.toLowerCase();
  return query.data.archived.includes(lower);
}

/**
 * Predicate for hiding archived identities from forward-looking discovery
 * surfaces (mention autocomplete, DM picker, member-adder, search,
 * panel-fold). Distinct from `useIsIdentityArchived` because callers here
 * need a synchronous boolean: while the `kind:13535` snapshot is loading the
 * predicate returns `false` (no-op — show everyone), never `true` — fail-open
 * so a cold-start can't briefly hide everyone.
 *
 * Self-exempt by construction: the current user is **never** filtered or
 * folded from their own client, even when archived on the relay. NIP-IA §Self
 * Requests makes archival deliberately non-silent — the anti-shadowban
 * property requires the archived user to see they're archived and be able to
 * self-unarchive. The profile pane's "Archived" flair is the honest
 * disclosure; removing self from member lists / autocomplete / search would
 * build the exact shadowban the NIP is designed to prevent. Self-exemption
 * lives here, in the predicate, so no caller can forget it.
 */
export function useIsArchivedPredicate(): (pubkey: string) => boolean {
  const query = useArchivedIdentitiesQuery();
  const identityQuery = useIdentityQuery();
  const selfPubkey = identityQuery.data?.pubkey;
  return React.useMemo(() => {
    const self = selfPubkey?.toLowerCase() ?? null;
    const set = new Set(
      (query.data?.archived ?? []).map((p) => p.toLowerCase()),
    );
    return (pubkey: string) => {
      const lower = pubkey.toLowerCase();
      return lower !== self && set.has(lower);
    };
  }, [query.data, selfPubkey]);
}

/**
 * Resolve the NIP-OA owner of a target via its live `kind:0`. Gates the
 * owner-path archive button.
 */
export function useOaOwnerQuery(pubkey: string, enabled = true) {
  return useQuery({
    enabled,
    queryKey: ["oaOwner", pubkey.toLowerCase()] as const,
    queryFn: () => resolveOaOwner(pubkey),
    staleTime: 60_000,
  });
}

export function useArchiveIdentityMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (req: IdentityArchiveRequest) => archiveIdentity(req),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: archivedIdentitiesQueryKey,
      });
    },
  });
}

export function useUnarchiveIdentityMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (req: IdentityUnarchiveRequest) => unarchiveIdentity(req),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: archivedIdentitiesQueryKey,
      });
    },
  });
}
