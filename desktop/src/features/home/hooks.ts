import { useQuery } from "@tanstack/react-query";

import { getHomeFeed } from "@/shared/api/tauri";

const homeFeedQueryKey = ["home-feed"] as const;

export function useHomeFeedQuery() {
  return useQuery({
    queryKey: homeFeedQueryKey,
    queryFn: () =>
      getHomeFeed({
        limit: 12,
        types: "mentions,needs_action,activity,agent_activity",
      }),
    staleTime: 15_000,
    gcTime: 5 * 60 * 1_000,
    refetchInterval: 30_000,
  });
}
