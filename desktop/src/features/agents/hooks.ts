import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  createManagedAgent,
  deleteManagedAgent,
  discoverAcpProviders,
  discoverManagedAgentPrereqs,
  getManagedAgentLog,
  listManagedAgents,
  listRelayAgents,
  mintManagedAgentToken,
  startManagedAgent,
  stopManagedAgent,
} from "@/shared/api/tauri";
import type {
  CreateManagedAgentInput,
  ManagedAgent,
  MintManagedAgentTokenInput,
} from "@/shared/api/types";

export const relayAgentsQueryKey = ["relay-agents"] as const;
export const managedAgentsQueryKey = ["managed-agents"] as const;
export const acpProvidersQueryKey = ["acp-providers"] as const;
export const managedAgentPrereqsQueryKey = ["managed-agent-prereqs"] as const;

export function useAcpProvidersQuery() {
  return useQuery({
    queryKey: acpProvidersQueryKey,
    queryFn: discoverAcpProviders,
    staleTime: 60_000,
  });
}

export function useManagedAgentPrereqsQuery(
  acpCommand: string,
  mcpCommand: string,
) {
  const normalizedAcpCommand = acpCommand.trim();
  const normalizedMcpCommand = mcpCommand.trim();

  return useQuery({
    queryKey: [
      ...managedAgentPrereqsQueryKey,
      normalizedAcpCommand,
      normalizedMcpCommand,
    ],
    queryFn: () =>
      discoverManagedAgentPrereqs({
        acpCommand: normalizedAcpCommand || undefined,
        mcpCommand: normalizedMcpCommand || undefined,
      }),
    staleTime: 15_000,
  });
}

export function useRelayAgentsQuery() {
  return useQuery({
    queryKey: relayAgentsQueryKey,
    queryFn: listRelayAgents,
    staleTime: 15_000,
  });
}

export function useManagedAgentsQuery() {
  return useQuery({
    queryKey: managedAgentsQueryKey,
    queryFn: listManagedAgents,
    staleTime: 1_000,
    refetchInterval: (query) => {
      const agents = query.state.data as ManagedAgent[] | undefined;
      return agents?.some((agent) => agent.status === "running")
        ? 2_000
        : 10_000;
    },
  });
}

export function useCreateManagedAgentMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: CreateManagedAgentInput) => createManagedAgent(input),
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: managedAgentsQueryKey });
      await queryClient.invalidateQueries({ queryKey: relayAgentsQueryKey });
    },
  });
}

export function useStartManagedAgentMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (pubkey: string) => startManagedAgent(pubkey),
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: managedAgentsQueryKey });
      await queryClient.invalidateQueries({ queryKey: relayAgentsQueryKey });
    },
  });
}

export function useStopManagedAgentMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (pubkey: string) => stopManagedAgent(pubkey),
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: managedAgentsQueryKey });
      await queryClient.invalidateQueries({ queryKey: relayAgentsQueryKey });
    },
  });
}

export function useDeleteManagedAgentMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (pubkey: string) => deleteManagedAgent(pubkey),
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: managedAgentsQueryKey });
      await queryClient.invalidateQueries({ queryKey: relayAgentsQueryKey });
    },
  });
}

export function useMintManagedAgentTokenMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: MintManagedAgentTokenInput) =>
      mintManagedAgentToken(input),
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: managedAgentsQueryKey });
      await queryClient.invalidateQueries({ queryKey: relayAgentsQueryKey });
    },
  });
}

export function useManagedAgentLogQuery(
  pubkey: string | null,
  lineCount = 120,
) {
  return useQuery({
    queryKey: ["managed-agent-log", pubkey, lineCount],
    queryFn: () => {
      if (!pubkey) {
        throw new Error("No agent selected.");
      }

      return getManagedAgentLog(pubkey, lineCount);
    },
    enabled: pubkey !== null,
    staleTime: 1_000,
    refetchInterval: pubkey ? 2_000 : false,
  });
}
