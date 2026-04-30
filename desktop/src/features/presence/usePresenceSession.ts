/**
 * usePresenceSession — heartbeat, idle detection, and status management.
 *
 * Manages the authenticated user's own presence lifecycle:
 * - 60s heartbeat PUT to refresh the 90s Redis TTL
 * - Idle detection (5 min → "away")
 * - Document visibility tracking
 * - User preference persistence (localStorage)
 */

import * as React from "react";

import type { PresenceStatus } from "@/shared/api/types";

import { usePresenceQuery, useSetPresenceMutation } from "./hooks";

const PRESENCE_HEARTBEAT_INTERVAL_MS = 60_000;
const PRESENCE_IDLE_TIMEOUT_MS = 5 * 60_000;
const PRESENCE_STATUS_TICK_INTERVAL_MS = 30_000;
const PRESENCE_PREFERENCE_STORAGE_KEY = "sprout-presence-preference";

type PresencePreference = "auto" | "away" | "offline" | null;

function presencePreferenceStorageKey(pubkey: string) {
  return `${PRESENCE_PREFERENCE_STORAGE_KEY}:${pubkey}`;
}

function readStoredPresencePreference(pubkey: string): PresencePreference {
  if (typeof window === "undefined" || pubkey.length === 0) {
    return null;
  }

  const value = window.localStorage.getItem(
    presencePreferenceStorageKey(pubkey),
  );
  return value === "auto" || value === "away" || value === "offline"
    ? value
    : null;
}

function writeStoredPresencePreference(
  pubkey: string,
  preference: PresencePreference,
) {
  if (typeof window === "undefined" || pubkey.length === 0) {
    return;
  }

  if (preference === null) {
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
          : presencePreference === "auto"
            ? automaticStatus
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
