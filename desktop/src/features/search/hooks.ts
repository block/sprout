import { useQuery } from "@tanstack/react-query";

import { searchMessages } from "@/shared/api/tauri";

export function useSearchMessagesQuery(
  query: string,
  options?: {
    enabled?: boolean;
    limit?: number;
  },
) {
  const trimmedQuery = query.trim();
  const enabled = options?.enabled ?? true;
  const limit = options?.limit ?? 12;

  return useQuery({
    queryKey: ["search-messages", trimmedQuery, limit],
    queryFn: () =>
      searchMessages({
        q: trimmedQuery,
        limit,
      }),
    enabled: enabled && trimmedQuery.length >= 2,
    staleTime: 30_000,
    gcTime: 5 * 60 * 1_000,
  });
}
