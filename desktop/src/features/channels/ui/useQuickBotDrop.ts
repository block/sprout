import * as React from "react";

import {
  useAcpProvidersQuery,
  useCreateChannelManagedAgentMutation,
} from "@/features/agents/hooks";
import { resolvePersonaProvider } from "@/features/agents/lib/resolvePersonaProvider";
import type { AgentPersona } from "@/shared/api/types";

type QuickBotDropState = {
  pending: boolean;
  error: string | null;
};

/**
 * Handles creating a new managed agent from a persona with a given instance name.
 */
export function useQuickBotDrop(channelId: string | null) {
  const createMutation = useCreateChannelManagedAgentMutation(channelId);
  const providersQuery = useAcpProvidersQuery();
  const [state, setState] = React.useState<QuickBotDropState>({
    pending: false,
    error: null,
  });

  const providers = providersQuery.data ?? [];
  const defaultProvider = providers[0] ?? null;

  const addBot = React.useCallback(
    async (persona: AgentPersona, instanceName: string) => {
      if (state.pending || !channelId) return;

      setState({ pending: true, error: null });

      try {
        const { provider } = resolvePersonaProvider(
          persona.provider,
          providers,
          defaultProvider,
        );

        if (!provider) {
          setState({
            pending: false,
            error: "No agent runtime available.",
          });
          return;
        }

        await createMutation.mutateAsync({
          provider,
          name: instanceName,
          systemPrompt: persona.systemPrompt,
          avatarUrl: persona.avatarUrl ?? undefined,
          personaId: persona.id,
          model: persona.model ?? undefined,
        });

        setState({ pending: false, error: null });
      } catch (err) {
        setState({
          pending: false,
          error: err instanceof Error ? err.message : "Failed to create agent.",
        });
      }
    },
    [channelId, createMutation, defaultProvider, providers, state.pending],
  );

  return { ...state, addBot };
}
