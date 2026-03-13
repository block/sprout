import type { Channel, RelayEvent } from "@/shared/api/types";

import type { TimelineMessage } from "@/features/messages/types";
import { getThreadReference } from "@/features/messages/lib/threading";
import {
  resolveUserLabel,
  type UserProfileLookup,
} from "@/features/profile/lib/identity";

function getEffectiveAuthorPubkey(event: RelayEvent) {
  const [firstTag] = event.tags;
  if (
    firstTag?.[0] === "p" &&
    firstTag[1] &&
    event.tags.some((tag) => tag[0] === "h")
  ) {
    return firstTag[1];
  }

  return event.pubkey;
}

function formatMessageAuthor(
  event: RelayEvent,
  channel: Channel | null,
  currentPubkey: string | undefined,
  profiles: UserProfileLookup | undefined,
) {
  const authorPubkey = getEffectiveAuthorPubkey(event);
  const fallbackName =
    channel?.channelType === "dm"
      ? (() => {
          const participantIndex =
            channel.participantPubkeys.indexOf(authorPubkey);
          if (participantIndex < 0) {
            return null;
          }

          return channel.participants[participantIndex] ?? null;
        })()
      : null;

  return resolveUserLabel({
    pubkey: authorPubkey,
    currentPubkey,
    fallbackName,
    profiles,
    preferResolvedSelfLabel: true,
  });
}

function getAuthorAvatarUrl(input: {
  authorPubkey: string;
  currentPubkey: string | undefined;
  currentUserAvatarUrl: string | null;
  profiles: UserProfileLookup | undefined;
}) {
  const { authorPubkey, currentPubkey, currentUserAvatarUrl, profiles } = input;

  if (currentPubkey === authorPubkey) {
    return currentUserAvatarUrl ?? null;
  }

  return profiles?.[authorPubkey.toLowerCase()]?.avatarUrl ?? null;
}

export function formatTimelineMessages(
  events: RelayEvent[],
  channel: Channel | null,
  currentPubkey: string | undefined,
  currentUserAvatarUrl: string | null,
  profiles?: UserProfileLookup,
): TimelineMessage[] {
  const eventsById = new Map(events.map((event) => [event.id, event]));
  const authorPubkeyByEventId = new Map<string, string>();
  const authorLabelByEventId = new Map<string, string>();
  const depthByEventId = new Map<string, number>();
  const resolvingEventIds = new Set<string>();

  function getAuthorLabel(event: RelayEvent) {
    const cached = authorLabelByEventId.get(event.id);
    if (cached) {
      return cached;
    }

    const authorPubkey = getEffectiveAuthorPubkey(event);
    const author = formatMessageAuthor(event, channel, currentPubkey, profiles);

    authorPubkeyByEventId.set(event.id, authorPubkey);
    authorLabelByEventId.set(event.id, author);
    return author;
  }

  function getDepth(event: RelayEvent): number {
    const cached = depthByEventId.get(event.id);
    if (cached !== undefined) {
      return cached;
    }

    if (resolvingEventIds.has(event.id)) {
      return 0;
    }

    const thread = getThreadReference(event.tags);
    if (!thread.parentId) {
      depthByEventId.set(event.id, 0);
      return 0;
    }

    const parent = eventsById.get(thread.parentId);
    if (!parent) {
      const fallbackDepth =
        thread.rootId && thread.rootId !== thread.parentId ? 2 : 1;
      depthByEventId.set(event.id, fallbackDepth);
      return fallbackDepth;
    }

    resolvingEventIds.add(event.id);
    const depth = getDepth(parent) + 1;
    resolvingEventIds.delete(event.id);
    depthByEventId.set(event.id, depth);
    return depth;
  }

  return events.map((event) => {
    const author = getAuthorLabel(event);
    const authorPubkey =
      authorPubkeyByEventId.get(event.id) ?? getEffectiveAuthorPubkey(event);
    const thread = getThreadReference(event.tags);
    const parentEvent = thread.parentId
      ? eventsById.get(thread.parentId)
      : undefined;

    return {
      id: event.id,
      createdAt: event.created_at,
      pubkey: authorPubkey,
      author,
      avatarUrl: getAuthorAvatarUrl({
        authorPubkey,
        currentPubkey,
        currentUserAvatarUrl,
        profiles,
      }),
      time: new Intl.DateTimeFormat("en-US", {
        hour: "numeric",
        minute: "2-digit",
      }).format(new Date(event.created_at * 1_000)),
      body: event.content,
      parentId: thread.parentId,
      rootId: thread.rootId,
      depth: getDepth(event),
      replyToAuthor: parentEvent ? getAuthorLabel(parentEvent) : null,
      replyToSnippet: parentEvent?.content ?? null,
      accent: currentPubkey === authorPubkey,
      pending: event.pending,
      kind: event.kind,
      tags: event.tags,
    };
  });
}

export function collectMessageAuthorPubkeys(events: RelayEvent[]) {
  return [
    ...new Set(
      events.map((event) => getEffectiveAuthorPubkey(event).toLowerCase()),
    ),
  ];
}
