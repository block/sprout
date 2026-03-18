import { CircleDot, FileText, Hash } from "lucide-react";

import type { Channel, PresenceStatus } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/shared/ui/sidebar";

import { PresenceDot } from "@/features/presence/ui/PresenceBadge";

function SidebarChannelIcon({ channel }: { channel: Channel }) {
  if (channel.channelType === "dm") {
    return <CircleDot className="h-4 w-4" />;
  }

  if (channel.channelType === "forum") {
    return <FileText className="h-4 w-4" />;
  }

  return <Hash className="h-4 w-4" />;
}

export function ChannelMenuButton({
  channel,
  label,
  isActive,
  hasUnread,
  presenceStatus,
  onSelectChannel,
}: {
  channel: Channel;
  label?: string;
  isActive: boolean;
  hasUnread: boolean;
  presenceStatus?: PresenceStatus;
  onSelectChannel: (channelId: string) => void;
}) {
  const resolvedLabel = label ?? channel.name;

  return (
    <SidebarMenuButton
      className={cn(
        !isActive &&
          hasUnread &&
          "font-semibold text-sidebar-foreground hover:text-sidebar-foreground",
      )}
      data-testid={`channel-${channel.name}`}
      isActive={isActive}
      onClick={() => onSelectChannel(channel.id)}
      tooltip={resolvedLabel}
      type="button"
    >
      <SidebarChannelIcon channel={channel} />
      <span className="min-w-0 flex-1 truncate">{resolvedLabel}</span>
      <div className="ml-auto flex items-center gap-2">
        {presenceStatus ? (
          <PresenceDot
            className="h-2 w-2"
            data-testid={`channel-presence-${channel.name}`}
            status={presenceStatus}
          />
        ) : null}
        {hasUnread && !isActive ? (
          <span
            aria-hidden="true"
            className="h-2.5 w-2.5 shrink-0 rounded-full bg-primary"
            data-testid={`channel-unread-${channel.name}`}
          />
        ) : null}
      </div>
    </SidebarMenuButton>
  );
}

export function SidebarSection({
  items,
  channelLabels,
  isActiveChannel,
  presenceByChannelId,
  selectedChannelId,
  title,
  testId,
  unreadChannelIds,
  onSelectChannel,
}: {
  items: Channel[];
  channelLabels?: Record<string, string>;
  isActiveChannel: boolean;
  presenceByChannelId?: Record<string, PresenceStatus>;
  selectedChannelId: string | null;
  title: string;
  testId: string;
  unreadChannelIds: Set<string>;
  onSelectChannel: (channelId: string) => void;
}) {
  if (items.length === 0) {
    return null;
  }

  return (
    <SidebarGroup>
      <SidebarGroupLabel>{title}</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu data-testid={testId}>
          {items.map((channel) => (
            <SidebarMenuItem key={channel.id}>
              <ChannelMenuButton
                channel={channel}
                hasUnread={unreadChannelIds.has(channel.id)}
                isActive={isActiveChannel && selectedChannelId === channel.id}
                label={channelLabels?.[channel.id] ?? channel.name}
                presenceStatus={presenceByChannelId?.[channel.id]}
                onSelectChannel={onSelectChannel}
              />
            </SidebarMenuItem>
          ))}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}
