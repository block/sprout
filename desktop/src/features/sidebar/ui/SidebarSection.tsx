import type * as React from "react";
import { CircleDot, FileText, Hash, Lock, X } from "lucide-react";

import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import type { Channel, PresenceStatus } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/shared/ui/sidebar";

import { PresenceDot } from "@/features/presence/ui/PresenceBadge";

export type SidebarDmParticipant = {
  avatarUrl: string | null;
  label: string;
  pubkey: string;
};

function DmChannelIcon({
  channelName,
  isPair,
  participants,
  presenceStatus,
}: {
  channelName: string;
  isPair: boolean;
  participants?: SidebarDmParticipant[];
  presenceStatus?: PresenceStatus;
}) {
  const primaryParticipant = participants?.[0];
  const secondaryParticipant = participants?.[1];

  if (!primaryParticipant) {
    return <CircleDot className="h-4 w-4" />;
  }

  if (isPair || !secondaryParticipant) {
    return (
      <span className="relative flex h-5 w-5 shrink-0 items-center justify-center">
        <ProfileAvatar
          avatarUrl={primaryParticipant.avatarUrl}
          className="h-5 w-5 rounded-full border border-sidebar-border/80 bg-sidebar-accent/80 text-[9px] text-sidebar-foreground shadow-none"
          iconClassName="h-3 w-3"
          label={primaryParticipant.label}
        />
        {presenceStatus ? (
          <span className="absolute -bottom-0.5 -right-0.5 flex h-2.5 w-2.5 items-center justify-center rounded-full bg-sidebar">
            <PresenceDot
              className="h-1.5 w-1.5"
              data-testid={`channel-presence-${channelName}`}
              status={presenceStatus}
            />
          </span>
        ) : null}
      </span>
    );
  }

  return (
    <span className="relative flex h-5 w-7 shrink-0 items-center">
      <ProfileAvatar
        avatarUrl={primaryParticipant.avatarUrl}
        className="absolute left-0 top-0 h-[18px] w-[18px] rounded-full border-2 border-sidebar bg-sidebar-accent/80 text-[8px] text-sidebar-foreground shadow-none"
        iconClassName="h-2.5 w-2.5"
        label={primaryParticipant.label}
      />
      <ProfileAvatar
        avatarUrl={secondaryParticipant.avatarUrl}
        className="absolute bottom-0 right-0 h-[18px] w-[18px] rounded-full border-2 border-sidebar bg-sidebar-accent/80 text-[8px] text-sidebar-foreground shadow-none"
        iconClassName="h-2.5 w-2.5"
        label={secondaryParticipant.label}
      />
      {participants && participants.length > 2 ? (
        <span className="absolute -bottom-1 -right-1 flex h-3.5 min-w-3.5 items-center justify-center rounded-full bg-sidebar-primary px-1 text-[8px] font-semibold text-sidebar-primary-foreground">
          {participants.length}
        </span>
      ) : null}
    </span>
  );
}

function SidebarChannelIcon({
  channel,
  dmParticipants,
  presenceStatus,
}: {
  channel: Channel;
  dmParticipants?: SidebarDmParticipant[];
  presenceStatus?: PresenceStatus;
}) {
  if (channel.channelType === "dm") {
    return (
      <DmChannelIcon
        channelName={channel.name}
        isPair={channel.participantPubkeys.length === 2}
        participants={dmParticipants}
        presenceStatus={
          dmParticipants?.length === 1 ||
          channel.participantPubkeys.length === 2
            ? presenceStatus
            : undefined
        }
      />
    );
  }

  if (channel.visibility === "private") {
    return <Lock className="h-4 w-4" />;
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
  dmParticipants,
  presenceStatus,
  onSelectChannel,
}: {
  channel: Channel;
  label?: string;
  isActive: boolean;
  hasUnread: boolean;
  dmParticipants?: SidebarDmParticipant[];
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
      <SidebarChannelIcon
        channel={channel}
        dmParticipants={dmParticipants}
        presenceStatus={presenceStatus}
      />
      <span className="min-w-0 flex-1 truncate">{resolvedLabel}</span>
      {hasUnread && !isActive ? (
        <span
          aria-hidden="true"
          className="ml-auto h-2.5 w-2.5 shrink-0 rounded-full bg-primary group-hover/menu-item:hidden"
          data-testid={`channel-unread-${channel.name}`}
        />
      ) : null}
    </SidebarMenuButton>
  );
}

export function SidebarSection({
  action,
  dmParticipantsByChannelId,
  emptyState,
  items,
  channelLabels,
  isActiveChannel,
  presenceByChannelId,
  selectedChannelId,
  title,
  testId,
  unreadChannelIds,
  onHideDm,
  onSelectChannel,
}: {
  action?: React.ReactNode;
  dmParticipantsByChannelId?: Record<string, SidebarDmParticipant[]>;
  emptyState?: React.ReactNode;
  items: Channel[];
  channelLabels?: Record<string, string>;
  isActiveChannel: boolean;
  presenceByChannelId?: Record<string, PresenceStatus>;
  selectedChannelId: string | null;
  title: string;
  testId: string;
  unreadChannelIds: Set<string>;
  onHideDm?: (channelId: string) => void;
  onSelectChannel: (channelId: string) => void;
}) {
  if (items.length === 0 && !action && !emptyState) {
    return null;
  }

  return (
    <SidebarGroup>
      <SidebarGroupLabel>{title}</SidebarGroupLabel>
      {action}
      <SidebarGroupContent>
        {items.length > 0 ? (
          <SidebarMenu data-testid={testId}>
            {items.map((channel) => (
              <SidebarMenuItem className="group/menu-item" key={channel.id}>
                <ChannelMenuButton
                  channel={channel}
                  dmParticipants={dmParticipantsByChannelId?.[channel.id]}
                  hasUnread={unreadChannelIds.has(channel.id)}
                  isActive={isActiveChannel && selectedChannelId === channel.id}
                  label={channelLabels?.[channel.id] ?? channel.name}
                  presenceStatus={presenceByChannelId?.[channel.id]}
                  onSelectChannel={onSelectChannel}
                />
                {channel.channelType === "dm" && onHideDm ? (
                  <SidebarMenuAction
                    aria-label="Close direct message"
                    data-testid={`hide-dm-${channel.name}`}
                    onClick={() => onHideDm(channel.id)}
                    showOnHover
                  >
                    <X />
                  </SidebarMenuAction>
                ) : null}
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        ) : emptyState ? (
          <div
            className="px-2 py-1 text-sm text-sidebar-foreground/60"
            data-testid={`${testId}-empty`}
          >
            {emptyState}
          </div>
        ) : null}
      </SidebarGroupContent>
    </SidebarGroup>
  );
}
