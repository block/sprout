import type { Channel, RelayEvent } from "@/shared/api/types";

import type {
  TimelineMessage,
  TimelineReaction,
} from "@/features/messages/types";
import { getThreadReference } from "@/features/messages/lib/threading";
import {
  resolveUserLabel,
  type UserProfileLookup,
} from "@/features/profile/lib/identity";
import {
  KIND_DELETION,
  KIND_REACTION,
  KIND_STREAM_MESSAGE,
  KIND_STREAM_MESSAGE_DIFF,
} from "@/shared/constants/kinds";

function isTimelineContentEvent(event: RelayEvent) {
  return (
    event.kind === KIND_STREAM_MESSAGE ||
    event.kind === KIND_STREAM_MESSAGE_DIFF
  );
}

function getDeletionTargets(tags: string[][]) {
  return tags
    .filter(
      (tag) =>
        tag[0] === "e" &&
        typeof tag[1] === "string" &&
        tag[1].length === 64 &&
        /^[0-9a-f]+$/i.test(tag[1]),
    )
    .map((tag) => tag[1]);
}

function getReactionTargetId(tags: string[][]) {
  for (let index = tags.length - 1; index >= 0; index -= 1) {
    const tag = tags[index];
    if (
      tag?.[0] === "e" &&
      typeof tag[1] === "string" &&
      tag[1].length === 64 &&
      /^[0-9a-f]+$/i.test(tag[1])
    ) {
      return tag[1];
    }
  }

  return null;
}

function getEffectiveAuthorPubkey(event: RelayEvent) {
  const actorTag = event.tags.find((tag) => tag[0] === "actor")?.[1];
  if (actorTag) {
    return actorTag;
  }

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
  const currentPubkeyLower = currentPubkey?.toLowerCase();
  const deletedEventIds = new Set<string>();
  for (const event of events) {
    if (event.kind !== KIND_DELETION) {
      continue;
    }

    for (const targetId of getDeletionTargets(event.tags)) {
      deletedEventIds.add(targetId);
    }
  }

  const visibleEvents = events.filter(
    (event) => isTimelineContentEvent(event) && !deletedEventIds.has(event.id),
  );
  const eventsById = new Map(visibleEvents.map((event) => [event.id, event]));
  const reactionPresence = new Map<
    string,
    {
      targetId: string;
      actorPubkey: string;
      emoji: string;
    }
  >();

  for (const event of events) {
    if (event.kind !== KIND_REACTION || deletedEventIds.has(event.id)) {
      continue;
    }

    const targetId = getReactionTargetId(event.tags);
    if (!targetId || deletedEventIds.has(targetId)) {
      continue;
    }

    const actorPubkey = getEffectiveAuthorPubkey(event).toLowerCase();
    const emoji = event.content.trim() || "+";
    reactionPresence.set(`${targetId}:${actorPubkey}:${emoji}`, {
      targetId,
      actorPubkey,
      emoji,
    });
  }

  const reactionsByEventId = new Map<string, Map<string, TimelineReaction>>();
  for (const { targetId, actorPubkey, emoji } of reactionPresence.values()) {
    const current = reactionsByEventId.get(targetId) ?? new Map();
    const existing = current.get(emoji) ?? {
      emoji,
      count: 0,
      reactedByCurrentUser: false,
    };

    existing.count += 1;
    if (currentPubkeyLower && actorPubkey === currentPubkeyLower) {
      existing.reactedByCurrentUser = true;
    }

    current.set(emoji, existing);
    reactionsByEventId.set(targetId, current);
  }

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

  return visibleEvents.map((event) => {
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
      reactions: (() => {
        const reactions = reactionsByEventId.get(event.id);
        return reactions ? [...reactions.values()] : undefined;
      })(),
    };
  });
}

export function collectMessageAuthorPubkeys(events: RelayEvent[]) {
  return [
    ...new Set(
      events
        .filter(isTimelineContentEvent)
        .map((event) => getEffectiveAuthorPubkey(event).toLowerCase()),
    ),
  ];
}
