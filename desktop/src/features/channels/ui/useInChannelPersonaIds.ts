import * as React from "react";

import { useManagedAgentsQuery } from "@/features/agents/hooks";
import { useChannelMembersQuery } from "@/features/channels/hooks";
import { normalizePubkey } from "@/shared/lib/pubkey";

const EMPTY_MAP = new Map<string, number>() as ReadonlyMap<string, number>;

/**
 * Returns a `Set<string>` of persona IDs whose managed agents are already
 * members of the given channel. The query is only enabled when `enabled` is
 * true (e.g. when the dialog is open).
 */
export function useInChannelPersonaIds(
  channelId: string | null,
  enabled: boolean,
): ReadonlySet<string> {
  return new Set(useInChannelPersonaCounts(channelId, enabled).keys());
}

/**
 * Returns a `Map<personaId, instanceCount>` for personas whose managed agents
 * are members of the given channel. Useful for showing how many instances of
 * each persona are already present.
 */
export function useInChannelPersonaCounts(
  channelId: string | null,
  enabled: boolean,
): ReadonlyMap<string, number> {
  const membersQuery = useChannelMembersQuery(channelId, enabled);
  const managedAgentsQuery = useManagedAgentsQuery();

  return React.useMemo(() => {
    const members = membersQuery.data;
    const managedAgents = managedAgentsQuery.data;
    if (!members || !managedAgents) {
      return EMPTY_MAP;
    }

    const memberPubkeys = new Set(
      members.map((m) => normalizePubkey(m.pubkey)),
    );

    const counts = new Map<string, number>();
    for (const agent of managedAgents) {
      if (agent.personaId && memberPubkeys.has(normalizePubkey(agent.pubkey))) {
        counts.set(agent.personaId, (counts.get(agent.personaId) ?? 0) + 1);
      }
    }
    return counts;
  }, [membersQuery.data, managedAgentsQuery.data]);
}
