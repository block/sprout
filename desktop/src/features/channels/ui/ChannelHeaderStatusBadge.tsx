import type { EphemeralChannelDisplay } from "@/features/channels/lib/ephemeralChannel";
import { EphemeralChannelBadge } from "@/features/channels/ui/EphemeralChannelBadge";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import type { Channel, PresenceStatus } from "@/shared/api/types";

type ChannelHeaderStatusBadgeProps = {
  channelType?: Channel["channelType"];
  ephemeralDisplay: EphemeralChannelDisplay | null;
  presenceStatus: PresenceStatus | null;
};

export function ChannelHeaderStatusBadge({
  channelType,
  ephemeralDisplay,
  presenceStatus,
}: ChannelHeaderStatusBadgeProps) {
  const ephemeralBadge = ephemeralDisplay ? (
    <EphemeralChannelBadge
      display={ephemeralDisplay}
      testId="chat-ephemeral-badge"
      variant="header"
    />
  ) : null;

  if (channelType === "dm" && presenceStatus) {
    return (
      <>
        <PresenceBadge
          data-testid="chat-presence-badge"
          status={presenceStatus}
        />
        {ephemeralBadge}
      </>
    );
  }

  return ephemeralBadge;
}
