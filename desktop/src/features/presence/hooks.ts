import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { relayClient } from "@/shared/api/relayClient";
import { getPresence, setPresence } from "@/shared/api/tauri";
import type { PresenceLookup, PresenceStatus } from "@/shared/api/types";

const PRESENCE_HEARTBEAT_INTERVAL_MS = 60_000;
const PRESENCE_TTL_SECONDS = 90;

function normalizePubkeys(pubkeys: string[]) {
  return [...new Set(pubkeys.map((pubkey) => pubkey.trim().toLowerCase()))]
    .filter((pubkey) => pubkey.length > 0)
    .sort();
}

function presenceQueryKey(pubkeys: string[]) {
  return ["presence", ...normalizePubkeys(pubkeys)] as const;
}

export function usePresenceQuery(
  pubkeys: string[],
  options?: {
    enabled?: boolean;
  },
) {
  const normalizedPubkeys = normalizePubkeys(pubkeys);
  const enabled = (options?.enabled ?? true) && normalizedPubkeys.length > 0;

  return useQuery<PresenceLookup>({
    enabled,
    queryKey: presenceQueryKey(normalizedPubkeys),
    queryFn: () => getPresence(normalizedPubkeys),
    staleTime: 15_000,
    refetchInterval: 30_000,
  });
}

export function useSetPresenceMutation(pubkey?: string) {
  const queryClient = useQueryClient();
  const normalizedPubkey = pubkey?.trim().toLowerCase() ?? "";

  return useMutation({
    mutationFn: async (status: PresenceStatus) => {
      try {
        return await setPresence(status);
      } catch (error) {
        if (
          !(error instanceof Error) ||
          (!error.message.includes("relay returned 404") &&
            !error.message.includes("relay returned 405"))
        ) {
          throw error;
        }

        await relayClient.sendPresence(status);

        return {
          status,
          ttlSeconds: status === "offline" ? 0 : PRESENCE_TTL_SECONDS,
        };
      }
    },
    onSuccess: ({ status }) => {
      if (normalizedPubkey.length === 0) {
        return;
      }

      queryClient.setQueryData<PresenceLookup>(
        presenceQueryKey([normalizedPubkey]),
        (current = {}) => ({
          ...current,
          [normalizedPubkey]: status,
        }),
      );
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: ["presence"] });
    },
  });
}

export function usePresenceSession(pubkey?: string) {
  const normalizedPubkey = pubkey?.trim().toLowerCase() ?? "";
  const presenceQuery = usePresenceQuery(
    normalizedPubkey.length > 0 ? [normalizedPubkey] : [],
    { enabled: normalizedPubkey.length > 0 },
  );
  const setPresenceMutation = useSetPresenceMutation(normalizedPubkey);
  const [desiredStatus, setDesiredStatus] =
    React.useState<PresenceStatus | null>(null);
  const previousPubkeyRef = React.useRef(normalizedPubkey);

  React.useEffect(() => {
    if (previousPubkeyRef.current === normalizedPubkey) {
      return;
    }

    previousPubkeyRef.current = normalizedPubkey;
    setDesiredStatus(null);
  });

  const relayStatus =
    normalizedPubkey.length > 0
      ? (presenceQuery.data?.[normalizedPubkey] ?? "offline")
      : "offline";
  const currentStatus = desiredStatus ?? relayStatus;

  const updatePresence = React.useCallback(
    async (status: PresenceStatus) => {
      const previousStatus = currentStatus;
      setDesiredStatus(status);

      try {
        await setPresenceMutation.mutateAsync(status);
      } catch (error) {
        setDesiredStatus(
          previousStatus === relayStatus ? null : previousStatus,
        );
        throw error;
      }
    },
    [currentStatus, relayStatus, setPresenceMutation],
  );

  const sendHeartbeat = React.useEffectEvent((status: PresenceStatus) => {
    void setPresenceMutation.mutateAsync(status).catch(() => {
      return;
    });
  });

  React.useEffect(() => {
    if (
      normalizedPubkey.length === 0 ||
      desiredStatus === null ||
      desiredStatus === "offline"
    ) {
      return;
    }

    const intervalId = window.setInterval(() => {
      sendHeartbeat(desiredStatus);
    }, PRESENCE_HEARTBEAT_INTERVAL_MS);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [desiredStatus, normalizedPubkey]);

  return {
    currentStatus,
    isLoading: presenceQuery.isLoading,
    isPending: setPresenceMutation.isPending,
    error:
      setPresenceMutation.error instanceof Error
        ? setPresenceMutation.error
        : presenceQuery.error instanceof Error
          ? presenceQuery.error
          : null,
    setStatus: updatePresence,
  };
}
