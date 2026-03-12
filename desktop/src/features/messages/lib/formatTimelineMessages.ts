import type { Channel, RelayEvent } from "@/shared/api/types";

import type { TimelineMessage } from "@/features/messages/types";
import {
  resolveUserLabel,
  type UserProfileLookup,
} from "@/features/profile/lib/identity";

function formatMessageAuthor(
  event: RelayEvent,
  channel: Channel | null,
  currentPubkey: string | undefined,
  profiles: UserProfileLookup | undefined,
) {
  const fallbackName =
    channel?.channelType === "dm"
      ? (() => {
          const participantIndex = channel.participantPubkeys.indexOf(
            event.pubkey,
          );
          if (participantIndex < 0) {
            return null;
          }

          return channel.participants[participantIndex] ?? null;
        })()
      : null;

  return resolveUserLabel({
    pubkey: event.pubkey,
    currentPubkey,
    fallbackName,
    profiles,
    preferResolvedSelfLabel: true,
  });
}

export function formatTimelineMessages(
  events: RelayEvent[],
  channel: Channel | null,
  currentPubkey: string | undefined,
  currentUserAvatarUrl: string | null,
  profiles?: UserProfileLookup,
): TimelineMessage[] {
  return events.map((event) => ({
    id: event.id,
    pubkey: event.pubkey,
    author: formatMessageAuthor(event, channel, currentPubkey, profiles),
    avatarUrl:
      currentPubkey === event.pubkey ? (currentUserAvatarUrl ?? null) : null,
    time: new Intl.DateTimeFormat("en-US", {
      hour: "numeric",
      minute: "2-digit",
    }).format(new Date(event.created_at * 1_000)),
    body: event.content,
    accent: currentPubkey === event.pubkey,
    pending: event.pending,
    kind: event.kind,
    tags: event.tags,
  }));
}

export function collectMessageAuthorPubkeys(events: RelayEvent[]) {
  return [...new Set(events.map((event) => event.pubkey.toLowerCase()))];
}
