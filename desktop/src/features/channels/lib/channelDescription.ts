import type { Channel } from "@/shared/api/types";

export function getChannelDescription(channel: Channel | null): string {
  if (!channel) {
    return "Connect to the relay to browse channels and read messages.";
  }

  return (
    [
      channel.archivedAt ? "Archived." : null,
      !channel.isMember ? "Read-only until you join this open channel." : null,
      channel.topic,
      channel.description,
      channel.purpose,
      null,
    ]
      .filter((value) => value && value.trim().length > 0)
      .join(" ") || "Channel details and activity."
  );
}
