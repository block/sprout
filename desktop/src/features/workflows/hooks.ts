import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  createWorkflow,
  deleteWorkflow,
  denyApproval,
  getChannelWorkflows,
  getWorkflow,
  getWorkflowRuns,
  grantApproval,
  triggerWorkflow,
  updateWorkflow,
} from "@/shared/api/tauriWorkflows";

export const workflowsQueryKey = (channelId: string) =>
  ["workflows", channelId] as const;
export const workflowQueryKey = (workflowId: string) =>
  ["workflow", workflowId] as const;
export const workflowRunsQueryKey = (workflowId: string) =>
  ["workflow-runs", workflowId] as const;

export function useChannelWorkflowsQuery(channelId: string | null) {
  return useQuery({
    queryKey: workflowsQueryKey(channelId ?? ""),
    queryFn: () => getChannelWorkflows(channelId!),
    enabled: channelId !== null,
    staleTime: 30_000,
    refetchInterval: 30_000,
  });
}

export function useWorkflowQuery(workflowId: string | null) {
  return useQuery({
    queryKey: workflowQueryKey(workflowId ?? ""),
    queryFn: () => getWorkflow(workflowId!),
    enabled: workflowId !== null,
    staleTime: 30_000,
  });
}

export function useWorkflowRunsQuery(workflowId: string | null) {
  return useQuery({
    queryKey: workflowRunsQueryKey(workflowId ?? ""),
    queryFn: () => getWorkflowRuns(workflowId!),
    enabled: workflowId !== null,
    staleTime: 10_000,
    refetchInterval: 10_000,
  });
}

export function useCreateWorkflowMutation(channelId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (yamlDefinition: string) =>
      createWorkflow(channelId, yamlDefinition),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: workflowsQueryKey(channelId),
      });
    },
  });
}

export function useUpdateWorkflowMutation(
  workflowId: string,
  channelId: string,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (yamlDefinition: string) =>
      updateWorkflow(workflowId, yamlDefinition),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: workflowQueryKey(workflowId),
      });
      void queryClient.invalidateQueries({
        queryKey: workflowsQueryKey(channelId),
      });
    },
  });
}

export function useDeleteWorkflowMutation(
  workflowId: string,
  channelId: string,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => deleteWorkflow(workflowId),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: workflowsQueryKey(channelId),
      });
    },
  });
}

export function useTriggerWorkflowMutation(workflowId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => triggerWorkflow(workflowId),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: workflowRunsQueryKey(workflowId),
      });
    },
  });
}

export function useApprovalMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: {
      token: string;
      action: "grant" | "deny";
      note?: string;
    }) =>
      input.action === "grant"
        ? grantApproval(input.token, input.note)
        : denyApproval(input.token, input.note),
    onSuccess: (_data, _variables) => {
      void queryClient.invalidateQueries({
        predicate: (query) =>
          query.queryKey[0] === "workflow-runs" ||
          query.queryKey[0] === "workflow",
      });
    },
  });
}
