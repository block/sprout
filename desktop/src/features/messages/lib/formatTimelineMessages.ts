import type { Channel, RelayEvent } from "@/shared/api/types";

import type { TimelineMessage } from "@/features/messages/types";
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
  return events.map((event) => {
    const authorPubkey = getEffectiveAuthorPubkey(event);

    return {
      id: event.id,
      pubkey: authorPubkey,
      author: formatMessageAuthor(event, channel, currentPubkey, profiles),
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
