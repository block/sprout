import { createFileRoute } from "@tanstack/react-router";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useChannelsQuery } from "@/features/channels/hooks";
import type { FeedItem, SearchHit } from "@/shared/api/types";
import { useIdentityQuery } from "@/shared/api/hooks";
import { HomeScreen } from "@/features/home/ui/HomeScreen";
import {
  KIND_FORUM_COMMENT,
  KIND_FORUM_POST,
  KIND_JOB_ACCEPTED,
  KIND_JOB_CANCEL,
  KIND_JOB_ERROR,
  KIND_JOB_PROGRESS,
  KIND_JOB_REQUEST,
  KIND_JOB_RESULT,
  KIND_STREAM_MESSAGE,
  KIND_STREAM_MESSAGE_V2,
} from "@/shared/constants/kinds";

function canOpenFeedItemAsExactEvent(item: FeedItem) {
  return (
    item.kind === KIND_STREAM_MESSAGE ||
    item.kind === KIND_STREAM_MESSAGE_V2 ||
    item.kind === KIND_JOB_REQUEST ||
    item.kind === KIND_JOB_ACCEPTED ||
    item.kind === KIND_JOB_PROGRESS ||
    item.kind === KIND_JOB_RESULT ||
    item.kind === KIND_JOB_CANCEL ||
    item.kind === KIND_JOB_ERROR ||
    item.kind === KIND_FORUM_POST ||
    item.kind === KIND_FORUM_COMMENT
  );
}

function toFeedItemSearchHit(item: FeedItem): SearchHit {
  return {
    channelId: item.channelId,
    channelName: item.channelName || null,
    content: item.content,
    createdAt: item.createdAt,
    eventId: item.id,
    kind: item.kind,
    pubkey: item.pubkey,
    score: 0,
  };
}

export const Route = createFileRoute("/")({
  component: HomeRouteComponent,
});

function HomeRouteComponent() {
  const { goChannel, goPulse, openSearchHit } = useAppNavigation();
  const channelsQuery = useChannelsQuery();
  const identityQuery = useIdentityQuery();
  const channels = channelsQuery.data ?? [];
  const availableChannelIds = new Set(channels.map((channel) => channel.id));

  return (
    <HomeScreen
      availableChannelIds={availableChannelIds}
      currentPubkey={identityQuery.data?.pubkey}
      onOpenFeedItem={(item) => {
        if (!item.channelId) {
          return;
        }

        if (canOpenFeedItemAsExactEvent(item)) {
          void openSearchHit(toFeedItemSearchHit(item));
          return;
        }

        void goChannel(item.channelId);
      }}
      onOpenPulse={() => {
        void goPulse();
      }}
    />
  );
}
