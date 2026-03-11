import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  getProfile,
  getUserProfile,
  getUsersBatch,
  updateProfile,
} from "@/shared/api/tauri";
import type {
  Profile,
  UpdateProfileInput,
  UsersBatchResponse,
} from "@/shared/api/types";

export const profileQueryKey = ["profile"] as const;

export function useProfileQuery(enabled = true) {
  return useQuery({
    enabled,
    queryKey: profileQueryKey,
    queryFn: getProfile,
    staleTime: 30_000,
  });
}

export function useUserProfileQuery(pubkey?: string) {
  return useQuery({
    enabled: typeof pubkey === "string" && pubkey.length > 0,
    queryKey: ["user-profile", pubkey?.toLowerCase() ?? ""],
    queryFn: () => getUserProfile(pubkey),
    staleTime: 60_000,
  });
}

export function useUsersBatchQuery(
  pubkeys: string[],
  options?: {
    enabled?: boolean;
  },
) {
  const normalizedPubkeys = [
    ...new Set(pubkeys.map((pubkey) => pubkey.toLowerCase())),
  ]
    .filter((pubkey) => pubkey.length > 0)
    .sort();
  const enabled = (options?.enabled ?? true) && normalizedPubkeys.length > 0;

  return useQuery<UsersBatchResponse>({
    enabled,
    queryKey: ["users-batch", ...normalizedPubkeys],
    queryFn: () => getUsersBatch(normalizedPubkeys),
    staleTime: 60_000,
    gcTime: 5 * 60 * 1_000,
  });
}

export function useUpdateProfileMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: UpdateProfileInput) => updateProfile(input),
    onSuccess: (profile: Profile) => {
      queryClient.setQueryData(profileQueryKey, profile);
    },
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: profileQueryKey });
    },
  });
}
