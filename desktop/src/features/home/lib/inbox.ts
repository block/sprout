import {
  resolveUserLabel,
  type UserProfileLookup,
} from "@/features/profile/lib/identity";
import type { FeedItem, HomeFeedResponse } from "@/shared/api/types";
import { resolveMentionNames } from "@/shared/lib/resolveMentionNames";

export type InboxFilter = "all" | "mention" | "needs_action";

export type InboxItem = {
  avatarUrl: string | null;
  id: string;
  item: FeedItem;
  categoryLabel: string;
  channelLabel: string | null;
  fullTimestampLabel: string;
  isActionRequired: boolean;
  mentionNames: string[];
  preview: string;
  searchableText: string;
  senderLabel: string;
  subject: string;
  timestampLabel: string;
};

export type InboxReply = {
  authorLabel: string;
  avatarUrl: string | null;
  content: string;
  fullTimestampLabel: string;
  id: string;
};

export type InboxGroup = {
  label: string;
  items: InboxItem[];
};

const listTimeFormatter = new Intl.DateTimeFormat("en-US", {
  hour: "numeric",
  minute: "2-digit",
});

const fullTimeFormatter = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  year: "numeric",
  hour: "numeric",
  minute: "2-digit",
});

const shortDateFormatter = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
});

const shortDateWithYearFormatter = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  year: "numeric",
});

const weekdayFormatter = new Intl.DateTimeFormat("en-US", {
  weekday: "long",
});

function startOfDay(value: Date) {
  return new Date(value.getFullYear(), value.getMonth(), value.getDate());
}

function diffInDays(from: Date, to: Date) {
  return Math.round(
    (startOfDay(from).getTime() - startOfDay(to).getTime()) / 86_400_000,
  );
}

function feedHeadline(item: FeedItem) {
  switch (item.kind) {
    case 40007:
      return "Reminder";
    case 43001:
      return "Job requested";
    case 43002:
      return "Job accepted";
    case 43003:
      return "Progress update";
    case 43004:
      return "Job result";
    case 43005:
      return "Job cancelled";
    case 43006:
      return "Job failed";
    case 45001:
      return "Forum post";
    case 45003:
      return "Forum reply";
    case 46010:
      return "Approval requested";
    default:
      if (item.category === "mention") {
        return "Mention";
      }

      if (item.category === "agent_activity") {
        return "Agent update";
      }

      return "Channel update";
  }
}

function feedPreview(item: FeedItem) {
  const content = item.content.trim();
  if (content.length > 0) {
    return content;
  }

  if (item.kind === 46010) {
    return "A workflow is waiting for approval.";
  }

  if (item.kind === 40007) {
    return "A reminder is waiting for you.";
  }

  return "No additional details were attached to this event.";
}

function formatInboxTimestamp(unixSeconds: number) {
  const date = new Date(unixSeconds * 1_000);
  const now = new Date();
  const dayDiff = diffInDays(now, date);

  if (dayDiff === 0) {
    return listTimeFormatter.format(date);
  }

  if (dayDiff === 1) {
    return "Yesterday";
  }

  if (now.getFullYear() === date.getFullYear()) {
    return shortDateFormatter.format(date);
  }

  return shortDateWithYearFormatter.format(date);
}

export function formatInboxFullTimestamp(unixSeconds: number) {
  return fullTimeFormatter.format(new Date(unixSeconds * 1_000));
}

export function groupInboxItems(items: InboxItem[]): InboxGroup[] {
  const groups = new Map<string, InboxItem[]>();
  const now = new Date();

  for (const item of items) {
    const date = new Date(item.item.createdAt * 1_000);
    const dayDiff = diffInDays(now, date);
    const label =
      dayDiff === 0
        ? "Today"
        : dayDiff === 1
          ? "Yesterday"
          : dayDiff < 7
            ? weekdayFormatter.format(date)
            : shortDateWithYearFormatter.format(date);

    const current = groups.get(label) ?? [];
    current.push(item);
    groups.set(label, current);
  }

  return [...groups.entries()].map(([label, groupedItems]) => ({
    label,
    items: groupedItems,
  }));
}

export function buildInboxItems({
  currentPubkey,
  feed,
  profiles,
}: {
  currentPubkey?: string;
  feed?: HomeFeedResponse;
  profiles?: UserProfileLookup;
}): InboxItem[] {
  if (!feed) {
    return [];
  }

  const items = [...feed.feed.mentions, ...feed.feed.needsAction].sort(
    (left, right) => right.createdAt - left.createdAt,
  );

  return items.map((item) => {
    const senderLabel = resolveUserLabel({
      pubkey: item.pubkey,
      currentPubkey,
      profiles,
      preferResolvedSelfLabel: true,
    });
    const subject = feedHeadline(item);
    const preview = feedPreview(item);
    const mentionNames = resolveMentionNames(item.tags, profiles) ?? [];
    const channelLabel = item.channelName.trim() || null;
    const categoryLabel =
      item.category === "needs_action" ? "Needs Action" : "Mention";

    return {
      avatarUrl: profiles?.[item.pubkey.toLowerCase()]?.avatarUrl ?? null,
      id: item.id,
      item,
      categoryLabel,
      channelLabel,
      fullTimestampLabel: formatInboxFullTimestamp(item.createdAt),
      isActionRequired: item.category === "needs_action",
      mentionNames,
      preview,
      searchableText: [
        senderLabel,
        subject,
        preview,
        channelLabel ?? "",
        categoryLabel,
      ]
        .join(" ")
        .toLowerCase(),
      senderLabel,
      subject,
      timestampLabel: formatInboxTimestamp(item.createdAt),
    };
  });
}
