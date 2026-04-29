import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import type { EphemeralChannelDisplay } from "@/features/channels/lib/ephemeralChannel";
import { getChannelDescription } from "@/features/channels/lib/channelDescription";
import { ChannelHeaderStatusBadge } from "@/features/channels/ui/ChannelHeaderStatusBadge";
import { ChannelMembersBar } from "@/features/channels/ui/ChannelMembersBar";
import type { Channel, PresenceStatus } from "@/shared/api/types";

type ChannelScreenHeaderProps = {
  activeChannel: Channel | null;
  activeChannelEphemeralDisplay: EphemeralChannelDisplay | null;
  activeChannelTitle: string;
  activeDmPresenceStatus: PresenceStatus | null;
  currentPubkey?: string;
  onManageChannel: () => void;
  onToggleMembers: () => void;
};

export function ChannelScreenHeader({
  activeChannel,
  activeChannelEphemeralDisplay,
  activeChannelTitle,
  activeDmPresenceStatus,
  currentPubkey,
  onManageChannel,
  onToggleMembers,
}: ChannelScreenHeaderProps) {
  return (
    <ChatHeader
      actions={
        activeChannel ? (
          <ChannelMembersBar
            channel={activeChannel}
            currentPubkey={currentPubkey}
            onManageChannel={onManageChannel}
            onToggleMembers={onToggleMembers}
          />
        ) : null
      }
      channelType={activeChannel?.channelType}
      description={getChannelDescription(activeChannel)}
      statusBadge={
        <ChannelHeaderStatusBadge
          channelType={activeChannel?.channelType}
          ephemeralDisplay={activeChannelEphemeralDisplay}
          presenceStatus={activeDmPresenceStatus}
        />
      }
      title={activeChannelTitle}
      visibility={activeChannel?.visibility}
    />
  );
}
