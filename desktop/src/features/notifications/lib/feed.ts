import type { FeedItem, HomeFeedResponse } from "@/shared/api/types";

const FEED_NOTIFICATION_BODY_MAX_LENGTH = 140;

export function notificationTitle(item: FeedItem) {
  const channelLabel = item.channelName.trim()
    ? ` in #${item.channelName.trim()}`
    : "";

  if (item.category === "mention") {
    return `@Mention${channelLabel}`;
  }

  if (item.kind === 46010) {
    return `Approval Requested${channelLabel}`;
  }

  return `Needs Action${channelLabel}`;
}

export function notificationBody(item: FeedItem) {
  const content = item.content.trim();
  const fallback =
    item.kind === 46010
      ? "A workflow is waiting for your approval."
      : "Something in Sprout needs your attention.";
  const body = content.length > 0 ? content : fallback;

  if (body.length <= FEED_NOTIFICATION_BODY_MAX_LENGTH) {
    return body;
  }

  return `${body.slice(0, FEED_NOTIFICATION_BODY_MAX_LENGTH - 3).trimEnd()}...`;
}

export function collectHomeAlertItems(feed: HomeFeedResponse) {
  return [...feed.feed.mentions, ...feed.feed.needsAction];
}

export function eligibleFeedNotificationItems(
  feed: HomeFeedResponse,
  options: { mentions: boolean; needsAction: boolean },
) {
  const items: FeedItem[] = [];

  if (options.mentions) {
    items.push(...feed.feed.mentions);
  }

  if (options.needsAction) {
    items.push(...feed.feed.needsAction);
  }

  return items.sort((left, right) => left.createdAt - right.createdAt);
}
