import * as React from "react";

import {
  useManagedAgentsQuery,
  useRelayAgentsQuery,
} from "@/features/agents/hooks";
import type { ChannelMember } from "@/shared/api/types";
import { normalizePubkey } from "@/shared/lib/pubkey";

import { compareMembersByRole } from "./memberUtils";

export function useClassifiedMembers(
  members: ChannelMember[],
  currentPubkey?: string,
) {
  const managedAgentsQuery = useManagedAgentsQuery();
  const relayAgentsQuery = useRelayAgentsQuery();

  const managedAgents = managedAgentsQuery.data ?? [];
  const relayAgents = relayAgentsQuery.data ?? [];

  const managedAgentPubkeys = React.useMemo(
    () => new Set(managedAgents.map((agent) => normalizePubkey(agent.pubkey))),
    [managedAgents],
  );
  const relayAgentPubkeys = React.useMemo(
    () => new Set(relayAgents.map((agent) => normalizePubkey(agent.pubkey))),
    [relayAgents],
  );

  const isBot = React.useCallback(
    (member: ChannelMember) => {
      const normalized = normalizePubkey(member.pubkey);
      return (
        member.role === "bot" ||
        managedAgentPubkeys.has(normalized) ||
        relayAgentPubkeys.has(normalized)
      );
    },
    [managedAgentPubkeys, relayAgentPubkeys],
  );

  const { people, bots } = React.useMemo(() => {
    const peopleList: ChannelMember[] = [];
    const botList: ChannelMember[] = [];

    for (const member of members) {
      if (isBot(member)) {
        botList.push(member);
      } else {
        peopleList.push(member);
      }
    }

    const sort = (list: ChannelMember[]) =>
      [...list].sort((left, right) =>
        compareMembersByRole(left, right, currentPubkey),
      );

    return { people: sort(peopleList), bots: sort(botList) };
  }, [currentPubkey, isBot, members]);

  return {
    people,
    bots,
    peopleCount: people.length,
    botCount: bots.length,
    isBot,
    managedAgentsQuery,
    relayAgentsQuery,
  };
}
