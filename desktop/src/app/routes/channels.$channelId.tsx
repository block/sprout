import * as React from "react";
import { createFileRoute } from "@tanstack/react-router";

import { getCachedSearchHitEvent } from "@/app/navigation/searchHitEventCache";
import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useChannelsQuery } from "@/features/channels/hooks";
import { ChannelScreen } from "@/features/channels/ui/ChannelScreen";
import { useProfileQuery } from "@/features/profile/hooks";
import { useIdentityQuery } from "@/shared/api/hooks";
import { getEventById } from "@/shared/api/tauri";
import type { RelayEvent } from "@/shared/api/types";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

type ChannelRouteSearch = {
  messageId?: string;
};

function validateChannelSearch(
  search: Record<string, unknown>,
): ChannelRouteSearch {
  return {
    messageId:
      typeof search.messageId === "string" && search.messageId.length > 0
        ? search.messageId
        : undefined,
  };
}

export const Route = createFileRoute("/channels/$channelId")({
  validateSearch: validateChannelSearch,
  component: ChannelRouteComponent,
});

export function ChannelRouteScreen({
  channelId,
  selectedPostId,
  targetMessageId,
  targetReplyId,
}: {
  channelId: string;
  selectedPostId: string | null;
  targetMessageId: string | null;
  targetReplyId: string | null;
}) {
  const { closeForumPost, goForumPost } = useAppNavigation();
  const channelsQuery = useChannelsQuery();
  const identityQuery = useIdentityQuery();
  const profileQuery = useProfileQuery();
  const channels = channelsQuery.data ?? [];
  const activeChannel =
    channels.find((channel) => channel.id === channelId) ?? null;
  const [targetMessageEvent, setTargetMessageEvent] =
    React.useState<RelayEvent | null>(() =>
      getCachedSearchHitEvent(targetMessageId),
    );

  React.useEffect(() => {
    let isCancelled = false;

    if (!targetMessageId || selectedPostId) {
      setTargetMessageEvent(null);
      return () => {
        isCancelled = true;
      };
    }

    setTargetMessageEvent(getCachedSearchHitEvent(targetMessageId));
    void getEventById(targetMessageId)
      .then((event) => {
        if (!isCancelled) {
          setTargetMessageEvent(event);
        }
      })
      .catch((error) => {
        if (!isCancelled) {
          console.error(
            "Failed to load route target event",
            targetMessageId,
            error,
          );
        }
      });

    return () => {
      isCancelled = true;
    };
  }, [selectedPostId, targetMessageId]);

  if (channelsQuery.isPending && !activeChannel) {
    return <ViewLoadingFallback includeHeader kind="channel" />;
  }

  return (
    <ChannelScreen
      activeChannel={activeChannel}
      currentIdentity={identityQuery.data}
      currentProfile={profileQuery.data}
      onCloseForumPost={() => {
        void closeForumPost(channelId);
      }}
      onSelectForumPost={(postId) => {
        void goForumPost(channelId, postId);
      }}
      selectedForumPostId={selectedPostId}
      targetForumReplyId={targetReplyId}
      targetMessageEvent={targetMessageEvent}
      targetMessageId={targetMessageId}
    />
  );
}

function ChannelRouteComponent() {
  const { channelId } = Route.useParams();
  const search = Route.useSearch();

  return (
    <ChannelRouteScreen
      channelId={channelId}
      selectedPostId={null}
      targetMessageId={search.messageId ?? null}
      targetReplyId={null}
    />
  );
}
