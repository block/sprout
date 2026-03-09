import type { Channel, RelayEvent } from "@/shared/api/types";

import type { TimelineMessage } from "@/features/messages/types";

function truncatePubkey(pubkey: string) {
  return `${pubkey.slice(0, 8)}…${pubkey.slice(-4)}`;
}

function formatMessageAuthor(
  event: RelayEvent,
  channel: Channel | null,
  currentPubkey: string | undefined,
) {
  if (currentPubkey && event.pubkey === currentPubkey) {
    return "You";
  }

  if (channel?.channelType === "dm") {
    const participantIndex = channel.participantPubkeys.indexOf(event.pubkey);
    if (participantIndex >= 0) {
      return (
        channel.participants[participantIndex] ?? truncatePubkey(event.pubkey)
      );
    }
  }

  return truncatePubkey(event.pubkey);
}

export function formatTimelineMessages(
  events: RelayEvent[],
  channel: Channel | null,
  currentPubkey: string | undefined,
): TimelineMessage[] {
  return events.map((event) => ({
    id: event.id,
    author: formatMessageAuthor(event, channel, currentPubkey),
    role:
      currentPubkey && event.pubkey === currentPubkey
        ? "Local"
        : channel?.channelType === "dm"
          ? "Participant"
          : "Pubkey",
    time: new Intl.DateTimeFormat("en-US", {
      hour: "numeric",
      minute: "2-digit",
    }).format(new Date(event.created_at * 1_000)),
    body: event.content,
    accent: currentPubkey === event.pubkey,
    pending: event.pending,
  }));
}
