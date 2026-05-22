import * as React from "react";

import {
  useAcpProvidersQuery,
  useManagedAgentsQuery,
  usePersonasQuery,
  useTeamsQuery,
} from "@/features/agents/hooks";
import { useChannelMembersQuery } from "@/features/channels/hooks";
import { getActivePersonas } from "@/features/agents/lib/catalog";
import { useBotRecents } from "@/features/agents/lib/useBotRecents";
import {
  getUsableTeams,
  resolveTeamPersonas,
} from "@/features/agents/lib/teamPersonas";
import type { AgentPersona, ManagedAgent } from "@/shared/api/types";
import { normalizePubkey } from "@/shared/lib/pubkey";

// ── Types ─────────────────────────────────────────────────────────────────────

export type RunningAvailableItem = {
  kind: "running-available";
  agent: ManagedAgent;
  persona: AgentPersona | null;
  label: string;
  avatarUrl: string | null;
};

export type RunningInChannelItem = {
  kind: "running-in-channel";
  agent: ManagedAgent;
  persona: AgentPersona | null;
  label: string;
  avatarUrl: string | null;
};

export type PersonaItem = {
  kind: "persona";
  persona: AgentPersona;
  label: string;
  avatarUrl: string | null;
};

export type QuickAddAgentItem =
  | RunningAvailableItem
  | RunningInChannelItem
  | PersonaItem;

// ── Helpers ───────────────────────────────────────────────────────────────────

export function getItemKey(item: QuickAddAgentItem): string {
  switch (item.kind) {
    case "persona":
      return `persona:${item.persona.id}`;
    case "running-available":
    case "running-in-channel":
      return `agent:${item.agent.pubkey}`;
  }
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useQuickAddAgentItems(
  channelId: string | null,
  enabled: boolean,
) {
  const managedAgentsQuery = useManagedAgentsQuery();
  const personasQuery = usePersonasQuery();
  const providersQuery = useAcpProvidersQuery();
  const teamsQuery = useTeamsQuery();
  const membersQuery = useChannelMembersQuery(channelId, enabled);
  const { recentIds, pushRecent } = useBotRecents();

  const managedAgents = managedAgentsQuery.data ?? [];
  const personas = React.useMemo(
    () => getActivePersonas(personasQuery.data ?? []),
    [personasQuery.data],
  );
  const providers = providersQuery.data ?? [];
  const defaultProvider = providers[0] ?? null;
  const members = membersQuery.data ?? [];
  const teams = teamsQuery.data ?? [];

  const channelMemberPubkeys = React.useMemo(
    () => new Set(members.map((m) => normalizePubkey(m.pubkey))),
    [members],
  );

  const usableTeams = React.useMemo(() => {
    const allUsable = getUsableTeams(teams, personas);
    // Filter out teams whose members are ALL already in the channel
    return allUsable.filter((team) => {
      const resolution = resolveTeamPersonas(team, personas);
      return resolution.resolvedPersonas.some((persona) => {
        // Check if this persona has a running agent not in channel, or no agent at all
        const runningAgent = managedAgents.find(
          (a) =>
            a.personaId === persona.id &&
            (a.status === "running" || a.status === "deployed"),
        );
        if (runningAgent) {
          return !channelMemberPubkeys.has(
            normalizePubkey(runningAgent.pubkey),
          );
        }
        // Persona has no running agent — it's addable (will create new)
        return true;
      });
    });
  }, [teams, personas, managedAgents, channelMemberPubkeys]);

  const items: QuickAddAgentItem[] = React.useMemo(() => {
    const result: QuickAddAgentItem[] = [];

    const runningAvailable = managedAgents.filter(
      (agent) =>
        (agent.status === "running" || agent.status === "deployed") &&
        !channelMemberPubkeys.has(normalizePubkey(agent.pubkey)),
    );

    const runningInChannel = managedAgents.filter(
      (agent) =>
        (agent.status === "running" || agent.status === "deployed") &&
        channelMemberPubkeys.has(normalizePubkey(agent.pubkey)),
    );

    const personaIdsInChannel = new Set(
      managedAgents
        .filter((agent) =>
          channelMemberPubkeys.has(normalizePubkey(agent.pubkey)),
        )
        .map((agent) => agent.personaId)
        .filter((id): id is string => Boolean(id)),
    );

    const availablePersonas = personas.filter(
      (persona) =>
        !personaIdsInChannel.has(persona.id) &&
        !runningAvailable.some((agent) => agent.personaId === persona.id),
    );

    const sortedRunningAvailable = [...runningAvailable].sort((a, b) => {
      const aPersonaIdx = a.personaId ? recentIds.indexOf(a.personaId) : -1;
      const bPersonaIdx = b.personaId ? recentIds.indexOf(b.personaId) : -1;
      const aScore = aPersonaIdx >= 0 ? aPersonaIdx : 999;
      const bScore = bPersonaIdx >= 0 ? bPersonaIdx : 999;
      return aScore - bScore;
    });

    for (const agent of runningInChannel) {
      const persona = agent.personaId
        ? (personas.find((p) => p.id === agent.personaId) ?? null)
        : null;
      result.push({
        kind: "running-in-channel",
        agent,
        persona,
        label: agent.name,
        avatarUrl: persona?.avatarUrl ?? null,
      });
    }

    for (const agent of sortedRunningAvailable) {
      const persona = agent.personaId
        ? (personas.find((p) => p.id === agent.personaId) ?? null)
        : null;
      result.push({
        kind: "running-available",
        agent,
        persona,
        label: agent.name,
        avatarUrl: persona?.avatarUrl ?? null,
      });
    }

    const sortedPersonas = [...availablePersonas].sort((a, b) => {
      const aIdx = recentIds.indexOf(a.id);
      const bIdx = recentIds.indexOf(b.id);
      if (aIdx >= 0 && bIdx >= 0) return aIdx - bIdx;
      if (aIdx >= 0) return -1;
      if (bIdx >= 0) return 1;
      return a.displayName.localeCompare(b.displayName);
    });

    for (const persona of sortedPersonas) {
      result.push({
        kind: "persona",
        persona,
        label: persona.displayName,
        avatarUrl: persona.avatarUrl,
      });
    }

    return result;
  }, [managedAgents, personas, channelMemberPubkeys, recentIds]);

  const isLoading =
    managedAgentsQuery.isLoading ||
    personasQuery.isLoading ||
    providersQuery.isLoading;

  return {
    items,
    isLoading,
    managedAgents,
    personas,
    providers,
    defaultProvider,
    usableTeams,
    teamsQuery,
    pushRecent,
  };
}
