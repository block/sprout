import * as React from "react";

import {
  useAttachManagedAgentToChannelMutation,
  useCreateChannelManagedAgentMutation,
  useCreateChannelManagedAgentsMutation,
} from "@/features/agents/hooks";
import { useRemoveChannelMemberMutation } from "@/features/channels/hooks";
import { resolvePersonaProvider } from "@/features/agents/lib/resolvePersonaProvider";
import { pickBotName } from "@/features/agents/lib/pickBotName";
import { resolveTeamPersonas } from "@/features/agents/lib/teamPersonas";
import type {
  AcpProvider,
  AgentPersona,
  AgentTeam,
  ManagedAgent,
} from "@/shared/api/types";
import { getItemKey, type QuickAddAgentItem } from "./useQuickAddAgentItems";

function safeBotName(persona: AgentPersona, usedNames: Set<string>): string {
  const pool = persona.namePool ?? [];
  const name = pickBotName(pool, usedNames);
  if (name && name.trim().length > 0) return name;
  return persona.displayName || "Agent";
}

type UseQuickAddAgentActionsParams = {
  channelId: string | null;
  items: QuickAddAgentItem[];
  managedAgents: ManagedAgent[];
  personas: AgentPersona[];
  providers: AcpProvider[];
  defaultProvider: AcpProvider | null;
  usableTeams: AgentTeam[];
  pushRecent: (personaId: string) => void;
};

export function useQuickAddAgentActions({
  channelId,
  items,
  managedAgents,
  personas,
  providers,
  defaultProvider,
  usableTeams,
  pushRecent,
}: UseQuickAddAgentActionsParams) {
  const attachMutation = useAttachManagedAgentToChannelMutation(channelId);
  const createMutation = useCreateChannelManagedAgentMutation(channelId);
  const batchCreateMutation = useCreateChannelManagedAgentsMutation(channelId);
  const removeMutation = useRemoveChannelMemberMutation(channelId);

  const [pendingKey, setPendingKey] = React.useState<string | null>(null);
  const [errorMessage, setErrorMessage] = React.useState<string | null>(null);
  const [selectMode, setSelectMode] = React.useState(false);
  const [selectedKeys, setSelectedKeys] = React.useState<Set<string>>(
    new Set(),
  );
  const [selectedTeamIds, setSelectedTeamIds] = React.useState<Set<string>>(
    new Set(),
  );

  // Reset state when popover closes (caller should pass `open`)
  const reset = React.useCallback(() => {
    setPendingKey(null);
    setErrorMessage(null);
    setSelectMode(false);
    setSelectedKeys(new Set());
    setSelectedTeamIds(new Set());
  }, []);

  // ── Single-add handlers ─────────────────────────────────────────────────

  async function handleAddRunningAgent(agent: ManagedAgent) {
    if (!channelId) return;
    const key = `agent:${agent.pubkey}`;
    setPendingKey(key);
    setErrorMessage(null);
    try {
      await attachMutation.mutateAsync({ agent, ensureRunning: true });
      if (agent.personaId) pushRecent(agent.personaId);
      setPendingKey(null);
    } catch (err) {
      setErrorMessage(
        err instanceof Error ? err.message : "Failed to add agent.",
      );
      setPendingKey(null);
    }
  }

  async function handleAddPersona(persona: AgentPersona) {
    if (!channelId) return;
    const key = `persona:${persona.id}`;
    setPendingKey(key);
    setErrorMessage(null);
    const { provider } = resolvePersonaProvider(
      persona.provider,
      providers,
      defaultProvider,
    );
    if (!provider) {
      setErrorMessage("No agent runtime available.");
      setPendingKey(null);
      return;
    }
    const usedNames = new Set(managedAgents.map((a) => a.name));
    const instanceName = safeBotName(persona, usedNames);
    try {
      await createMutation.mutateAsync({
        provider,
        name: instanceName,
        systemPrompt: persona.systemPrompt,
        avatarUrl: persona.avatarUrl ?? undefined,
        personaId: persona.id,
        model: persona.model ?? undefined,
      });
      pushRecent(persona.id);
      setPendingKey(null);
    } catch (err) {
      setErrorMessage(
        err instanceof Error ? err.message : "Failed to add agent.",
      );
      setPendingKey(null);
    }
  }

  // ── Multi-select handlers ───────────────────────────────────────────────

  function handleCancelSelect() {
    setSelectMode(false);
    setSelectedKeys(new Set());
    setSelectedTeamIds(new Set());
  }

  function toggleSelection(key: string) {
    setSelectedKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      setSelectedTeamIds((prevTeams) => {
        const nextTeams = new Set(prevTeams);
        for (const team of usableTeams) {
          if (!nextTeams.has(team.id)) continue;
          const resolution = resolveTeamPersonas(team, personas);
          const allSelected = resolution.resolvedPersonas.every((p) => {
            const runningItem = items.find(
              (i) =>
                i.kind === "running-available" && i.agent.personaId === p.id,
            );
            const ik = runningItem
              ? getItemKey(runningItem)
              : (() => {
                  const pi = items.find(
                    (i) => i.kind === "persona" && i.persona.id === p.id,
                  );
                  return pi ? getItemKey(pi) : null;
                })();
            return ik ? next.has(ik) : true;
          });
          if (!allSelected) nextTeams.delete(team.id);
        }
        return nextTeams;
      });
      return next;
    });
  }

  function handleTeamToggle(team: AgentTeam, pressed: boolean) {
    const resolution = resolveTeamPersonas(team, personas);
    const memberKeys: string[] = [];
    for (const persona of resolution.resolvedPersonas) {
      const runningItem = items.find(
        (i) =>
          i.kind === "running-available" && i.agent.personaId === persona.id,
      );
      if (runningItem) {
        memberKeys.push(getItemKey(runningItem));
      } else {
        const personaItem = items.find(
          (i) => i.kind === "persona" && i.persona.id === persona.id,
        );
        if (personaItem) memberKeys.push(getItemKey(personaItem));
      }
    }
    setSelectedTeamIds((prev) => {
      const next = new Set(prev);
      if (pressed) next.add(team.id);
      else next.delete(team.id);
      return next;
    });
    setSelectedKeys((prev) => {
      const next = new Set(prev);
      for (const key of memberKeys) {
        if (pressed) next.add(key);
        else next.delete(key);
      }
      return next;
    });
  }

  async function handleBatchAdd() {
    if (!channelId || selectedKeys.size === 0) return;
    setPendingKey("batch");
    setErrorMessage(null);
    const usedNames = new Set(managedAgents.map((a) => a.name));
    const toAttach: ManagedAgent[] = [];
    const toCreate: Array<{ persona: AgentPersona; instanceName: string }> = [];
    for (const key of selectedKeys) {
      const item = items.find((i) => getItemKey(i) === key);
      if (!item || item.kind === "running-in-channel") continue;
      if (item.kind === "running-available") {
        toAttach.push(item.agent);
      } else {
        const instanceName = safeBotName(item.persona, usedNames);
        usedNames.add(instanceName);
        toCreate.push({ persona: item.persona, instanceName });
      }
    }
    try {
      for (const agent of toAttach) {
        await attachMutation.mutateAsync({ agent, ensureRunning: true });
        if (agent.personaId) pushRecent(agent.personaId);
      }
      if (toCreate.length > 0 && defaultProvider) {
        const inputs = toCreate.map(({ persona, instanceName }) => {
          const { provider } = resolvePersonaProvider(
            persona.provider,
            providers,
            defaultProvider,
          );
          const providerToUse = provider ?? defaultProvider;
          return {
            provider: {
              id: providerToUse.id,
              label: providerToUse.label,
              command: providerToUse.command,
              defaultArgs: providerToUse.defaultArgs,
              mcpCommand: providerToUse.mcpCommand,
            },
            name: instanceName,
            systemPrompt: persona.systemPrompt,
            avatarUrl: persona.avatarUrl ?? undefined,
            personaId: persona.id,
            model: persona.model ?? undefined,
          };
        });
        await batchCreateMutation.mutateAsync(inputs);
        for (const { persona } of toCreate) pushRecent(persona.id);
      }
      setPendingKey(null);
      setSelectMode(false);
      setSelectedKeys(new Set());
      setSelectedTeamIds(new Set());
    } catch (err) {
      setErrorMessage(
        err instanceof Error ? err.message : "Failed to add agents.",
      );
      setPendingKey(null);
    }
  }

  // ── Item click dispatcher ───────────────────────────────────────────────

  function handleItemClick(item: QuickAddAgentItem) {
    if (item.kind === "running-in-channel") return;
    if (pendingKey) return;
    if (!channelId) return;
    if (selectMode) {
      toggleSelection(getItemKey(item));
    } else if (item.kind === "running-available") {
      void handleAddRunningAgent(item.agent);
    } else {
      void handleAddPersona(item.persona);
    }
  }

  return {
    pendingKey,
    errorMessage,
    selectMode,
    setSelectMode,
    selectedKeys,
    selectedTeamIds,
    reset,
    handleCancelSelect,
    handleTeamToggle,
    handleBatchAdd,
    handleItemClick,
    removeMutation,
  };
}
