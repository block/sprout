import * as React from "react";

import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import {
  useChannelsQuery,
  useSelectedChannel,
} from "@/features/channels/hooks";
import { MessageComposer } from "@/features/chat/ui/MessageComposer";
import { MessageTimeline } from "@/features/chat/ui/MessageTimeline";
import {
  useChannelMessagesQuery,
  useChannelSubscription,
  useSendMessageMutation,
} from "@/features/messages/hooks";
import { formatTimelineMessages } from "@/features/messages/lib/formatTimelineMessages";
import { AppSidebar } from "@/features/sidebar/ui/AppSidebar";
import { useIdentityQuery } from "@/shared/api/hooks";
import { SidebarInset, SidebarProvider } from "@/shared/ui/sidebar";

export function AppShell() {
  const identityQuery = useIdentityQuery();
  const channelsQuery = useChannelsQuery();
  const channels = channelsQuery.data ?? [];
  const { selectedChannel, setSelectedChannelId } = useSelectedChannel(
    channels,
    null,
  );

  const messagesQuery = useChannelMessagesQuery(selectedChannel);
  useChannelSubscription(selectedChannel);

  const sendMessageMutation = useSendMessageMutation(
    selectedChannel,
    identityQuery.data,
  );

  const timelineMessages = React.useMemo(
    () =>
      formatTimelineMessages(
        messagesQuery.data ?? [],
        selectedChannel,
        identityQuery.data?.pubkey,
      ),
    [identityQuery.data?.pubkey, messagesQuery.data, selectedChannel],
  );

  const channelDescription = selectedChannel
    ? selectedChannel.channelType === "forum"
      ? `${selectedChannel.description} Forum channels are listed, but this first pass only wires message streams and DMs.`
      : selectedChannel.description
    : "Connect to the relay to browse channels and read messages.";

  return (
    <SidebarProvider className="h-dvh overflow-hidden overscroll-none">
      <AppSidebar
        channels={channels}
        errorMessage={
          channelsQuery.error instanceof Error
            ? channelsQuery.error.message
            : undefined
        }
        isLoading={channelsQuery.isLoading}
        onSelectChannel={(channelId) => {
          React.startTransition(() => setSelectedChannelId(channelId));
        }}
        selectedChannelId={selectedChannel?.id ?? null}
      />

      <SidebarInset className="min-h-0 min-w-0 overflow-hidden">
        <ChatHeader
          channelType={selectedChannel?.channelType}
          description={channelDescription}
          title={selectedChannel?.name ?? "Channels"}
        />

        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <MessageTimeline
            emptyDescription={
              selectedChannel?.channelType === "forum"
                ? "Select a stream or DM to load real message history in this first integration pass."
                : "Messages will appear here once the relay has history for this channel."
            }
            emptyTitle={
              selectedChannel
                ? selectedChannel.channelType === "forum"
                  ? "Forum channels are next"
                  : "No messages yet"
                : "No channel selected"
            }
            isLoading={messagesQuery.isLoading}
            messages={timelineMessages}
          />
          <MessageComposer
            channelName={selectedChannel?.name ?? "channel"}
            disabled={
              !selectedChannel ||
              selectedChannel.channelType === "forum" ||
              sendMessageMutation.isPending
            }
            isSending={sendMessageMutation.isPending}
            key={selectedChannel?.id ?? "no-channel"}
            onSend={async (content) => {
              await sendMessageMutation.mutateAsync(content);
            }}
            placeholder={
              selectedChannel?.channelType === "forum"
                ? "Forum posting is not wired in this pass."
                : selectedChannel
                  ? `Message #${selectedChannel.name}`
                  : "Select a channel"
            }
          />
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
