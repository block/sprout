import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { relayClient } from "@/shared/api/relayClient";
import { getPresence, setPresence } from "@/shared/api/tauri";
import type { PresenceLookup, PresenceStatus } from "@/shared/api/types";

const PRESENCE_HEARTBEAT_INTERVAL_MS = 60_000;
const PRESENCE_IDLE_TIMEOUT_MS = 5 * 60_000;
const PRESENCE_STATUS_TICK_INTERVAL_MS = 30_000;
const PRESENCE_TTL_SECONDS = 90;
const PRESENCE_PREFERENCE_STORAGE_KEY = "sprout-presence-preference";

type PresencePreference = "auto" | "away" | "offline";

function normalizePubkeys(pubkeys: string[]) {
  return [...new Set(pubkeys.map((pubkey) => pubkey.trim().toLowerCase()))]
    .filter((pubkey) => pubkey.length > 0)
    .sort();
}

function presenceQueryKey(pubkeys: string[]) {
  return ["presence", ...normalizePubkeys(pubkeys)] as const;
}

function presencePreferenceStorageKey(pubkey: string) {
  return `${PRESENCE_PREFERENCE_STORAGE_KEY}:${pubkey}`;
}

function readStoredPresencePreference(pubkey: string): PresencePreference {
  if (typeof window === "undefined" || pubkey.length === 0) {
    return "auto";
  }

  const value = window.localStorage.getItem(
    presencePreferenceStorageKey(pubkey),
  );
  return value === "away" || value === "offline" ? value : "auto";
}

function writeStoredPresencePreference(
  pubkey: string,
  preference: PresencePreference,
) {
  if (typeof window === "undefined" || pubkey.length === 0) {
    return;
  }

  if (preference === "auto") {
    window.localStorage.removeItem(presencePreferenceStorageKey(pubkey));
    return;
  }

  window.localStorage.setItem(presencePreferenceStorageKey(pubkey), preference);
}

function resolveAutomaticPresenceStatus(
  isDocumentHidden: boolean,
  lastActivityAt: number,
  now: number,
): PresenceStatus {
  if (isDocumentHidden) {
    return "away";
  }

  return now - lastActivityAt >= PRESENCE_IDLE_TIMEOUT_MS ? "away" : "online";
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
  const [presencePreference, setPresencePreference] =
    React.useState<PresencePreference>(() =>
      readStoredPresencePreference(normalizedPubkey),
    );
  const [lastActivityAt, setLastActivityAt] = React.useState(() => Date.now());
  const [statusClock, setStatusClock] = React.useState(() => Date.now());
  const [isDocumentHidden, setIsDocumentHidden] = React.useState(() =>
    typeof document === "undefined" ? false : document.hidden,
  );
  const skipNextSyncRef = React.useRef<PresenceStatus | null>(null);

  React.useEffect(() => {
    const now = Date.now();
    setPresencePreference(readStoredPresencePreference(normalizedPubkey));
    setLastActivityAt(now);
    setStatusClock(now);
    setIsDocumentHidden(
      typeof document === "undefined" ? false : document.hidden,
    );
  }, [normalizedPubkey]);

  React.useEffect(() => {
    writeStoredPresencePreference(normalizedPubkey, presencePreference);
  }, [normalizedPubkey, presencePreference]);

  const recordActivity = React.useEffectEvent(() => {
    const now = Date.now();
    setLastActivityAt(now);
    setStatusClock(now);
  });

  React.useEffect(() => {
    if (normalizedPubkey.length === 0) {
      return;
    }

    function handleUserActivity() {
      if (typeof document !== "undefined" && document.hidden) {
        return;
      }

      recordActivity();
    }

    function handleFocus() {
      setIsDocumentHidden(false);
      recordActivity();
    }

    function handleVisibilityChange() {
      const hidden = document.hidden;
      setIsDocumentHidden(hidden);

      if (!hidden) {
        recordActivity();
      }
    }

    window.addEventListener("pointerdown", handleUserActivity, true);
    window.addEventListener("keydown", handleUserActivity, true);
    window.addEventListener("focus", handleFocus);
    document.addEventListener("visibilitychange", handleVisibilityChange);

    return () => {
      window.removeEventListener("pointerdown", handleUserActivity, true);
      window.removeEventListener("keydown", handleUserActivity, true);
      window.removeEventListener("focus", handleFocus);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [normalizedPubkey]);

  React.useEffect(() => {
    if (normalizedPubkey.length === 0) {
      return;
    }

    const intervalId = window.setInterval(() => {
      setStatusClock(Date.now());
    }, PRESENCE_STATUS_TICK_INTERVAL_MS);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [normalizedPubkey]);

  const automaticStatus = React.useMemo(
    () =>
      resolveAutomaticPresenceStatus(
        isDocumentHidden,
        lastActivityAt,
        statusClock,
      ),
    [isDocumentHidden, lastActivityAt, statusClock],
  );
  const currentStatus =
    normalizedPubkey.length === 0
      ? "offline"
      : presencePreference === "offline"
        ? "offline"
        : presencePreference === "away"
          ? "away"
          : automaticStatus;

  const updatePresence = React.useCallback(
    async (status: PresenceStatus) => {
      const previousPreference = presencePreference;
      const nextPreference: PresencePreference =
        status === "online" ? "auto" : status;

      if (nextPreference === "auto") {
        const now = Date.now();
        setLastActivityAt(now);
        setStatusClock(now);
        setIsDocumentHidden(
          typeof document === "undefined" ? false : document.hidden,
        );
      }

      setPresencePreference(nextPreference);
      skipNextSyncRef.current = status;

      try {
        await setPresenceMutation.mutateAsync(status);
      } catch (error) {
        skipNextSyncRef.current = null;
        setPresencePreference(previousPreference);
        throw error;
      }
    },
    [presencePreference, setPresenceMutation],
  );

  const syncPresence = React.useEffectEvent((status: PresenceStatus) => {
    void setPresenceMutation.mutateAsync(status).catch(() => {
      return;
    });
  });

  React.useEffect(() => {
    if (normalizedPubkey.length === 0) {
      return;
    }

    if (skipNextSyncRef.current === currentStatus) {
      skipNextSyncRef.current = null;
      return;
    }

    syncPresence(currentStatus);
  }, [currentStatus, normalizedPubkey]);

  React.useEffect(() => {
    if (normalizedPubkey.length === 0 || currentStatus === "offline") {
      return;
    }

    const intervalId = window.setInterval(() => {
      syncPresence(currentStatus);
    }, PRESENCE_HEARTBEAT_INTERVAL_MS);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [currentStatus, normalizedPubkey]);

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
