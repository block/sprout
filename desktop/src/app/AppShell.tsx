import * as React from "react";

import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import {
  useCreateChannelMutation,
  useChannelsQuery,
  useSelectedChannel,
} from "@/features/channels/hooks";
import { useHomeFeedQuery } from "@/features/home/hooks";
import { HomeView } from "@/features/home/ui/HomeView";
import {
  useChannelMessagesQuery,
  useChannelSubscription,
  mergeMessages,
  useSendMessageMutation,
} from "@/features/messages/hooks";
import { formatTimelineMessages } from "@/features/messages/lib/formatTimelineMessages";
import { MessageComposer } from "@/features/messages/ui/MessageComposer";
import { MessageTimeline } from "@/features/messages/ui/MessageTimeline";
import { SearchDialog } from "@/features/search/ui/SearchDialog";
import { AppSidebar } from "@/features/sidebar/ui/AppSidebar";
import { getEventById } from "@/shared/api/tauri";
import { useIdentityQuery } from "@/shared/api/hooks";
import type { RelayEvent, SearchHit } from "@/shared/api/types";
import { SidebarInset, SidebarProvider } from "@/shared/ui/sidebar";

type AppView = "home" | "channel";

function createSearchAnchorEvent(hit: SearchHit): RelayEvent {
  return {
    id: hit.eventId,
    pubkey: hit.pubkey,
    created_at: hit.createdAt,
    kind: hit.kind,
    tags: [["h", hit.channelId]],
    content: hit.content,
    sig: "",
  };
}

export function AppShell() {
  const [selectedView, setSelectedView] = React.useState<AppView>("home");
  const [isSearchOpen, setIsSearchOpen] = React.useState(false);
  const [searchAnchor, setSearchAnchor] = React.useState<SearchHit | null>(
    null,
  );
  const [searchAnchorChannelId, setSearchAnchorChannelId] = React.useState<
    string | null
  >(null);
  const [searchAnchorEvent, setSearchAnchorEvent] =
    React.useState<RelayEvent | null>(null);
  const identityQuery = useIdentityQuery();
  const homeFeedQuery = useHomeFeedQuery();
  const channelsQuery = useChannelsQuery();
  const channels = channelsQuery.data ?? [];
  const { selectedChannel, setSelectedChannelId } = useSelectedChannel(
    channels,
    null,
  );
  const createChannelMutation = useCreateChannelMutation();
  const activeChannel = selectedView === "channel" ? selectedChannel : null;

  const messagesQuery = useChannelMessagesQuery(activeChannel);
  useChannelSubscription(activeChannel);

  const sendMessageMutation = useSendMessageMutation(
    activeChannel,
    identityQuery.data,
  );
  const homeUrgentCount =
    (homeFeedQuery.data?.feed.mentions.length ?? 0) +
    (homeFeedQuery.data?.feed.needsAction.length ?? 0);
  const availableChannelIds = React.useMemo(
    () => new Set(channels.map((channel) => channel.id)),
    [channels],
  );
  const resolvedMessages = React.useMemo(() => {
    const currentMessages = messagesQuery.data ?? [];

    if (
      !activeChannel ||
      !searchAnchorEvent ||
      searchAnchorChannelId !== activeChannel.id
    ) {
      return currentMessages;
    }

    return mergeMessages(currentMessages, searchAnchorEvent);
  }, [
    activeChannel,
    messagesQuery.data,
    searchAnchorChannelId,
    searchAnchorEvent,
  ]);

  const timelineMessages = React.useMemo(
    () =>
      formatTimelineMessages(
        resolvedMessages,
        activeChannel,
        identityQuery.data?.pubkey,
      ),
    [activeChannel, identityQuery.data?.pubkey, resolvedMessages],
  );

  const channelDescription = activeChannel
    ? activeChannel.channelType === "forum"
      ? `${activeChannel.description} Forum channels are listed, but this first pass only wires message streams and DMs.`
      : activeChannel.description
    : "Connect to the relay to browse channels and read messages.";
  const contentPaneKey =
    selectedView === "home" ? "home" : `channel:${activeChannel?.id ?? "none"}`;
  const isTimelineLoading =
    messagesQuery.isLoading && resolvedMessages.length === 0;

  const handleOpenChannel = React.useCallback(
    (channelId: string) => {
      React.startTransition(() => {
        setSelectedChannelId(channelId);
        setSelectedView("channel");
      });
    },
    [setSelectedChannelId],
  );

  const handleOpenSearchResult = React.useCallback(
    (hit: SearchHit) => {
      setSearchAnchor(hit);
      setSearchAnchorChannelId(hit.channelId);
      setSearchAnchorEvent(createSearchAnchorEvent(hit));
      handleOpenChannel(hit.channelId);

      void getEventById(hit.eventId)
        .then((event) => {
          setSearchAnchorEvent((current) => {
            if (current?.id !== hit.eventId) {
              return current;
            }

            return event;
          });
        })
        .catch((error) => {
          console.error(
            "Failed to load search result event",
            hit.eventId,
            error,
          );
        });
    },
    [handleOpenChannel],
  );

  return (
    <SidebarProvider className="h-dvh overflow-hidden overscroll-none">
      <AppSidebar
        channels={channels}
        errorMessage={
          channelsQuery.error instanceof Error
            ? channelsQuery.error.message
            : undefined
        }
        homeUrgentCount={homeUrgentCount}
        isLoading={channelsQuery.isLoading}
        isCreatingChannel={createChannelMutation.isPending}
        onCreateChannel={async ({ description, name }) => {
          const createdChannel = await createChannelMutation.mutateAsync({
            name,
            description,
            channelType: "stream",
            visibility: "open",
          });

          React.startTransition(() => {
            setSelectedChannelId(createdChannel.id);
            setSelectedView("channel");
          });
        }}
        onOpenSearch={() => {
          setIsSearchOpen(true);
        }}
        onSelectHome={() => {
          React.startTransition(() => {
            setSelectedView("home");
          });

          void homeFeedQuery.refetch();
        }}
        onSelectChannel={handleOpenChannel}
        selectedChannelId={selectedChannel?.id ?? null}
        selectedView={selectedView}
      />

      <SidebarInset
        className="min-h-0 min-w-0 overflow-hidden"
        key={contentPaneKey}
      >
        {selectedView === "home" ? (
          <ChatHeader
            description="Personalized feed for mentions, reminders, channel activity, and agent work."
            mode="home"
            title="Home"
          />
        ) : (
          <ChatHeader
            channelType={activeChannel?.channelType}
            description={channelDescription}
            title={activeChannel?.name ?? "Channels"}
          />
        )}

        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          {selectedView === "home" ? (
            <HomeView
              availableChannelIds={availableChannelIds}
              currentPubkey={identityQuery.data?.pubkey}
              errorMessage={
                homeFeedQuery.error instanceof Error
                  ? homeFeedQuery.error.message
                  : undefined
              }
              feed={homeFeedQuery.data}
              isLoading={homeFeedQuery.isLoading}
              isRefreshing={homeFeedQuery.isRefetching}
              onOpenChannel={handleOpenChannel}
              onRefresh={() => {
                void homeFeedQuery.refetch();
              }}
            />
          ) : (
            <>
              <MessageTimeline
                emptyDescription={
                  activeChannel?.channelType === "forum"
                    ? "Select a stream or DM to load real message history in this first integration pass."
                    : "Messages will appear here once the relay has history for this channel."
                }
                emptyTitle={
                  activeChannel
                    ? activeChannel.channelType === "forum"
                      ? "Forum channels are next"
                      : "No messages yet"
                    : "No channel selected"
                }
                isLoading={isTimelineLoading}
                key={activeChannel?.id ?? "no-channel"}
                messages={timelineMessages}
                onTargetReached={(messageId) => {
                  setSearchAnchor((current) =>
                    current?.eventId === messageId ? null : current,
                  );
                }}
                targetMessageId={
                  activeChannel && searchAnchor?.channelId === activeChannel.id
                    ? searchAnchor.eventId
                    : null
                }
              />
              <MessageComposer
                channelName={activeChannel?.name ?? "channel"}
                disabled={
                  !activeChannel ||
                  activeChannel.channelType === "forum" ||
                  sendMessageMutation.isPending
                }
                isSending={sendMessageMutation.isPending}
                key={activeChannel?.id ?? "no-channel"}
                onSend={async (content) => {
                  await sendMessageMutation.mutateAsync(content);
                }}
                placeholder={
                  activeChannel?.channelType === "forum"
                    ? "Forum posting is not wired in this pass."
                    : activeChannel
                      ? `Message #${activeChannel.name}`
                      : "Select a channel"
                }
              />
            </>
          )}
        </div>

        <SearchDialog
          channels={channels}
          onOpenResult={handleOpenSearchResult}
          onOpenChange={setIsSearchOpen}
          open={isSearchOpen}
        />
      </SidebarInset>
    </SidebarProvider>
  );
}
