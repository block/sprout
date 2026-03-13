import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  listTokens,
  mintToken,
  revokeAllTokens,
  revokeToken,
} from "@/shared/api/tauri";
import type { MintTokenInput, Token } from "@/shared/api/types";

export const tokensQueryKey = ["tokens"] as const;

export function useTokensQuery() {
  return useQuery({
    queryKey: tokensQueryKey,
    queryFn: listTokens,
    staleTime: 30_000,
  });
}

export function useMintTokenMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: MintTokenInput) => mintToken(input),
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: tokensQueryKey });
    },
  });
}

export function useRevokeTokenMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (tokenId: string) => revokeToken(tokenId),
    onMutate: async (tokenId) => {
      await queryClient.cancelQueries({ queryKey: tokensQueryKey });
      const previous = queryClient.getQueryData<Token[]>(tokensQueryKey);

      queryClient.setQueryData<Token[]>(tokensQueryKey, (old) =>
        old?.map((t) =>
          t.id === tokenId ? { ...t, revokedAt: new Date().toISOString() } : t,
        ),
      );

      return { previous };
    },
    onError: (_err, _tokenId, context) => {
      if (context?.previous) {
        queryClient.setQueryData(tokensQueryKey, context.previous);
      }
    },
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: tokensQueryKey });
    },
  });
}

export function useRevokeAllTokensMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => revokeAllTokens(),
    onMutate: async () => {
      await queryClient.cancelQueries({ queryKey: tokensQueryKey });
      const previous = queryClient.getQueryData<Token[]>(tokensQueryKey);
      const now = new Date().toISOString();

      queryClient.setQueryData<Token[]>(tokensQueryKey, (old) =>
        old?.map((t) => (t.revokedAt ? t : { ...t, revokedAt: now })),
      );

      return { previous };
    },
    onError: (_err, _vars, context) => {
      if (context?.previous) {
        queryClient.setQueryData(tokensQueryKey, context.previous);
      }
    },
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: tokensQueryKey });
    },
  });
}
