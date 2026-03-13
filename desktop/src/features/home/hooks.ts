import { useQuery } from "@tanstack/react-query";

import { getHomeFeed } from "@/shared/api/tauri";

export function useHomeFeedQuery() {
  return useQuery({
    queryKey: ["home-feed"],
    queryFn: () => getHomeFeed({ limit: 12, types: "mentions,needs_action" }),
    staleTime: 15_000,
    gcTime: 5 * 60 * 1_000,
    refetchInterval: 30_000,
  });
}
