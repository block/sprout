import { useQuery } from "@tanstack/react-query";

import { searchMessages } from "@/shared/api/tauri";

export function useSearchMessagesQuery(
  query: string,
  options?: {
    channelId?: string;
    enabled?: boolean;
    limit?: number;
  },
) {
  const trimmedQuery = query.trim();
  const enabled = options?.enabled ?? true;
  const limit = options?.limit ?? 12;
  const channelId = options?.channelId;

  return useQuery({
    queryKey: ["search-messages", trimmedQuery, limit, channelId ?? null],
    queryFn: () =>
      searchMessages({
        q: trimmedQuery,
        limit,
        channelId,
      }),
    enabled: enabled && trimmedQuery.length >= 2,
    staleTime: 30_000,
    gcTime: 5 * 60 * 1_000,
  });
}
