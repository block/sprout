import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { getProfile, updateProfile } from "@/shared/api/tauri";
import type { Profile, UpdateProfileInput } from "@/shared/api/types";

export const profileQueryKey = ["profile"] as const;

export function useProfileQuery(enabled = true) {
  return useQuery({
    enabled,
    queryKey: profileQueryKey,
    queryFn: getProfile,
    staleTime: 30_000,
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
