import * as React from "react";

import { useHomeFeedQuery } from "@/features/home/hooks";
import type { FeedItem, HomeFeedResponse } from "@/shared/api/types";
import {
  collectHomeAlertItems,
  eligibleFeedNotificationItems,
  notificationBody,
  notificationTitle,
} from "./lib/feed";
import {
  getDesktopNotificationPermissionState,
  requestDesktopNotificationAccess,
  sendDesktopNotification,
  type DesktopNotificationPermissionState,
} from "./lib/desktop";

export type { DesktopNotificationPermissionState } from "./lib/desktop";

const NOTIFICATION_SETTINGS_STORAGE_KEY = "sprout-notification-settings.v1";
const HOME_FEED_SEEN_STORAGE_KEY = "sprout-home-feed-seen.v1";
const HOME_FEED_SEEN_MAX_ITEMS = 500;

export type NotificationSettings = {
  desktopEnabled: boolean;
  homeBadgeEnabled: boolean;
  mentions: boolean;
  needsAction: boolean;
};

const DEFAULT_NOTIFICATION_SETTINGS: NotificationSettings = {
  desktopEnabled: false,
  homeBadgeEnabled: true,
  mentions: true,
  needsAction: true,
};

function notificationSettingsStorageKey(pubkey: string) {
  return `${NOTIFICATION_SETTINGS_STORAGE_KEY}:${pubkey}`;
}

function homeFeedSeenStorageKey(pubkey: string) {
  return `${HOME_FEED_SEEN_STORAGE_KEY}:${pubkey}`;
}

function sanitizeNotificationSettings(value: unknown): NotificationSettings {
  if (!value || typeof value !== "object") {
    return DEFAULT_NOTIFICATION_SETTINGS;
  }

  const candidate = value as Partial<NotificationSettings>;
  return {
    desktopEnabled:
      typeof candidate.desktopEnabled === "boolean"
        ? candidate.desktopEnabled
        : DEFAULT_NOTIFICATION_SETTINGS.desktopEnabled,
    homeBadgeEnabled:
      typeof candidate.homeBadgeEnabled === "boolean"
        ? candidate.homeBadgeEnabled
        : DEFAULT_NOTIFICATION_SETTINGS.homeBadgeEnabled,
    mentions:
      typeof candidate.mentions === "boolean"
        ? candidate.mentions
        : DEFAULT_NOTIFICATION_SETTINGS.mentions,
    needsAction:
      typeof candidate.needsAction === "boolean"
        ? candidate.needsAction
        : DEFAULT_NOTIFICATION_SETTINGS.needsAction,
  };
}

function readStoredNotificationSettings(pubkey: string): NotificationSettings {
  if (typeof window === "undefined" || pubkey.length === 0) {
    return DEFAULT_NOTIFICATION_SETTINGS;
  }

  const rawValue = window.localStorage.getItem(
    notificationSettingsStorageKey(pubkey),
  );
  if (!rawValue) {
    return DEFAULT_NOTIFICATION_SETTINGS;
  }

  try {
    return sanitizeNotificationSettings(JSON.parse(rawValue));
  } catch {
    return DEFAULT_NOTIFICATION_SETTINGS;
  }
}

function writeStoredNotificationSettings(
  pubkey: string,
  settings: NotificationSettings,
) {
  if (typeof window === "undefined" || pubkey.length === 0) {
    return;
  }

  window.localStorage.setItem(
    notificationSettingsStorageKey(pubkey),
    JSON.stringify(settings),
  );
}

function readStoredSeenFeedIds(pubkey: string): string[] {
  if (typeof window === "undefined" || pubkey.length === 0) {
    return [];
  }

  const rawValue = window.localStorage.getItem(homeFeedSeenStorageKey(pubkey));
  if (!rawValue) {
    return [];
  }

  try {
    const parsed = JSON.parse(rawValue);
    if (!Array.isArray(parsed)) {
      return [];
    }

    return parsed
      .filter((value): value is string => typeof value === "string")
      .slice(-HOME_FEED_SEEN_MAX_ITEMS);
  } catch {
    return [];
  }
}

function writeStoredSeenFeedIds(pubkey: string, ids: string[]) {
  if (typeof window === "undefined" || pubkey.length === 0) {
    return;
  }

  window.localStorage.setItem(
    homeFeedSeenStorageKey(pubkey),
    JSON.stringify(ids.slice(-HOME_FEED_SEEN_MAX_ITEMS)),
  );
}

function mergeSeenFeedIds(current: string[], nextIds: readonly string[]) {
  const merged = new Set(current);
  let didChange = false;

  for (const id of nextIds) {
    if (merged.has(id)) {
      continue;
    }

    merged.add(id);
    didChange = true;
  }

  if (!didChange) {
    return current;
  }

  const values = [...merged];
  return values.length <= HOME_FEED_SEEN_MAX_ITEMS
    ? values
    : values.slice(values.length - HOME_FEED_SEEN_MAX_ITEMS);
}

export function useNotificationSettings(pubkey?: string) {
  const normalizedPubkey = pubkey?.trim().toLowerCase() ?? "";
  const [settings, setSettings] = React.useState<NotificationSettings>(() =>
    readStoredNotificationSettings(normalizedPubkey),
  );
  const [permission, setPermission] =
    React.useState<DesktopNotificationPermissionState>("default");
  const [errorMessage, setErrorMessage] = React.useState<string | null>(null);
  const [isUpdatingDesktopEnabled, setIsUpdatingDesktopEnabled] =
    React.useState(false);

  React.useEffect(() => {
    setSettings(readStoredNotificationSettings(normalizedPubkey));
    setErrorMessage(null);
  }, [normalizedPubkey]);

  React.useEffect(() => {
    writeStoredNotificationSettings(normalizedPubkey, settings);
  }, [normalizedPubkey, settings]);

  const refreshPermission = React.useEffectEvent(async () => {
    const nextPermission = await getDesktopNotificationPermissionState();
    setPermission(nextPermission);
    return nextPermission;
  });

  React.useEffect(() => {
    void normalizedPubkey;
    void refreshPermission();
  }, [normalizedPubkey]);

  const setDesktopEnabled = React.useCallback(async (enabled: boolean) => {
    if (!enabled) {
      setErrorMessage(null);
      setSettings((current) => ({
        ...current,
        desktopEnabled: false,
      }));
      void refreshPermission();
      return true;
    }

    setIsUpdatingDesktopEnabled(true);
    setErrorMessage(null);

    try {
      let nextPermission = await refreshPermission();
      if (nextPermission === "default") {
        nextPermission = await requestDesktopNotificationAccess();
        setPermission(nextPermission);
      }

      if (nextPermission !== "granted") {
        setSettings((current) => ({
          ...current,
          desktopEnabled: false,
        }));
        setErrorMessage(
          nextPermission === "denied"
            ? "Desktop notifications are blocked for Sprout. Enable them in system settings to turn alerts on."
            : "Desktop notifications are unavailable in this environment.",
        );
        return false;
      }

      setSettings((current) => ({
        ...current,
        desktopEnabled: true,
      }));
      return true;
    } catch (error) {
      setSettings((current) => ({
        ...current,
        desktopEnabled: false,
      }));
      setErrorMessage(
        error instanceof Error
          ? error.message
          : "Failed to enable desktop notifications.",
      );
      return false;
    } finally {
      setIsUpdatingDesktopEnabled(false);
    }
  }, []);

  const setHomeBadgeEnabled = React.useCallback((enabled: boolean) => {
    setSettings((current) => ({
      ...current,
      homeBadgeEnabled: enabled,
    }));
  }, []);

  const setMentionsEnabled = React.useCallback((enabled: boolean) => {
    setSettings((current) => ({
      ...current,
      mentions: enabled,
    }));
  }, []);

  const setNeedsActionEnabled = React.useCallback((enabled: boolean) => {
    setSettings((current) => ({
      ...current,
      needsAction: enabled,
    }));
  }, []);

  return {
    errorMessage,
    isUpdatingDesktopEnabled,
    permission,
    setDesktopEnabled,
    setHomeBadgeEnabled,
    setMentionsEnabled,
    setNeedsActionEnabled,
    settings,
  };
}

export function useFeedDesktopNotifications(
  feed: HomeFeedResponse | undefined,
  pubkey: string | undefined,
  settings: NotificationSettings,
) {
  const normalizedPubkey = pubkey?.trim().toLowerCase() ?? "";
  const seenItemIdsRef = React.useRef<Set<string>>(
    new Set(readStoredSeenFeedIds(normalizedPubkey)),
  );

  React.useEffect(() => {
    seenItemIdsRef.current = new Set(readStoredSeenFeedIds(normalizedPubkey));
  }, [normalizedPubkey]);

  const deliverFeedNotification = React.useEffectEvent(
    async (item: FeedItem) => {
      await sendDesktopNotification({
        body: notificationBody(item),
        target: {
          channelId: item.channelId,
          channelName: item.channelName,
          content: item.content,
          createdAt: item.createdAt,
          eventId: item.id,
          kind: item.kind,
          pubkey: item.pubkey,
        },
        title: notificationTitle(item),
      });
    },
  );

  React.useEffect(() => {
    if (!feed) {
      return;
    }

    const currentFeedItems = collectHomeAlertItems(feed);
    const nextSeenItemIds = new Set(seenItemIdsRef.current);
    const newItems = settings.desktopEnabled
      ? eligibleFeedNotificationItems(feed, {
          mentions: settings.mentions,
          needsAction: settings.needsAction,
        }).filter((item) => !nextSeenItemIds.has(item.id))
      : [];

    for (const item of currentFeedItems) {
      nextSeenItemIds.add(item.id);
    }

    // Prevent unbounded growth — keep only the most recent entries.
    const MAX_SEEN_FEED_ITEMS = 500;
    if (nextSeenItemIds.size > MAX_SEEN_FEED_ITEMS) {
      const excess = nextSeenItemIds.size - MAX_SEEN_FEED_ITEMS;
      let removed = 0;
      for (const id of nextSeenItemIds) {
        if (removed >= excess) break;
        nextSeenItemIds.delete(id);
        removed++;
      }
    }

    seenItemIdsRef.current = nextSeenItemIds;
    writeStoredSeenFeedIds(normalizedPubkey, [...nextSeenItemIds]);

    for (const item of newItems) {
      void deliverFeedNotification(item);
    }
  }, [
    feed,
    normalizedPubkey,
    settings.desktopEnabled,
    settings.mentions,
    settings.needsAction,
  ]);
}

export function useHomeFeedNotificationState(
  feed: HomeFeedResponse | undefined,
  pubkey: string | undefined,
  settings: NotificationSettings,
  isHomeActive: boolean,
) {
  useFeedDesktopNotifications(feed, pubkey, settings);
  const normalizedPubkey = pubkey?.trim().toLowerCase() ?? "";
  const [seenFeedIds, setSeenFeedIds] = React.useState<string[]>(() =>
    readStoredSeenFeedIds(normalizedPubkey),
  );
  const currentFeedIds = React.useMemo(
    () =>
      feed
        ? [...feed.feed.mentions, ...feed.feed.needsAction].map(
            (item) => item.id,
          )
        : [],
    [feed],
  );

  React.useEffect(() => {
    setSeenFeedIds(readStoredSeenFeedIds(normalizedPubkey));
  }, [normalizedPubkey]);

  React.useEffect(() => {
    writeStoredSeenFeedIds(normalizedPubkey, seenFeedIds);
  }, [normalizedPubkey, seenFeedIds]);

  const markCurrentFeedSeen = React.useEffectEvent(() => {
    setSeenFeedIds((current) => mergeSeenFeedIds(current, currentFeedIds));
  });

  React.useEffect(() => {
    if (!isHomeActive || currentFeedIds.length === 0) {
      return;
    }

    void normalizedPubkey;
    markCurrentFeedSeen();
  }, [currentFeedIds, isHomeActive, normalizedPubkey]);

  return React.useMemo(() => {
    if (!settings.homeBadgeEnabled || isHomeActive) {
      return 0;
    }

    if (currentFeedIds.length === 0) {
      return 0;
    }

    const seenFeedIdSet = new Set(seenFeedIds);
    return currentFeedIds.filter((id) => !seenFeedIdSet.has(id)).length;
  }, [currentFeedIds, isHomeActive, seenFeedIds, settings.homeBadgeEnabled]);
}

export function useHomeFeedNotifications(
  pubkey: string | undefined,
  isHomeActive: boolean,
) {
  const notificationSettings = useNotificationSettings(pubkey);
  const homeFeedQuery = useHomeFeedQuery();
  const refetchHomeFeedForE2e = React.useEffectEvent(() => {
    void homeFeedQuery.refetch();
  });

  React.useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }

    function handleMockHomeFeedUpdate() {
      refetchHomeFeedForE2e();
    }

    window.addEventListener(
      "sprout:e2e-home-feed-updated",
      handleMockHomeFeedUpdate,
    );
    return () => {
      window.removeEventListener(
        "sprout:e2e-home-feed-updated",
        handleMockHomeFeedUpdate,
      );
    };
  }, []);

  const homeBadgeCount = useHomeFeedNotificationState(
    homeFeedQuery.data,
    pubkey,
    notificationSettings.settings,
    isHomeActive,
  );

  return {
    homeBadgeCount,
    homeFeedQuery,
    notificationSettings,
  };
}
