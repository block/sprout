import { createFileRoute } from "@tanstack/react-router";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useChannelsQuery } from "@/features/channels/hooks";
import { useIdentityQuery } from "@/shared/api/hooks";
import { HomeScreen } from "@/features/home/ui/HomeScreen";

type HomeRouteSearch = {
  item?: string;
};

function validateHomeSearch(search: Record<string, unknown>): HomeRouteSearch {
  return {
    item:
      typeof search.item === "string" && search.item.length > 0
        ? search.item
        : undefined,
  };
}

export const Route = createFileRoute("/")({
  validateSearch: validateHomeSearch,
  component: HomeRouteComponent,
});

function HomeRouteComponent() {
  const { goChannel } = useAppNavigation();
  const channelsQuery = useChannelsQuery();
  const identityQuery = useIdentityQuery();
  const channels = channelsQuery.data ?? [];
  const availableChannelIds = new Set(channels.map((channel) => channel.id));

  return (
    <HomeScreen
      availableChannelIds={availableChannelIds}
      currentPubkey={identityQuery.data?.pubkey}
      onOpenChannel={(channelId) => {
        void goChannel(channelId);
      }}
      onOpenContext={(channelId, messageId) => {
        void goChannel(channelId, { messageId });
      }}
    />
  );
}
