import { RefreshCcw } from "lucide-react";

import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import { useHomeFeedQuery } from "@/features/home/hooks";
import { HomeView } from "@/features/home/ui/HomeView";
import { Button } from "@/shared/ui/button";

type HomeScreenProps = {
  availableChannelIds: ReadonlySet<string>;
  currentPubkey?: string;
  onOpenChannel: (channelId: string) => void;
};

export function HomeScreen({
  availableChannelIds,
  currentPubkey,
  onOpenChannel,
}: HomeScreenProps) {
  const homeFeedQuery = useHomeFeedQuery();

  return (
    <>
      <ChatHeader
        actions={
          <Button
            className="h-9 rounded-full px-3"
            onClick={() => {
              void homeFeedQuery.refetch();
            }}
            type="button"
            variant="outline"
          >
            <RefreshCcw
              className={`h-4 w-4 ${homeFeedQuery.isFetching ? "animate-spin" : ""}`}
            />
            Refresh
          </Button>
        }
        description="Personalized inbox for mentions, reminders, and approvals."
        mode="home"
        title="Inbox"
      />

      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <HomeView
          availableChannelIds={availableChannelIds}
          currentPubkey={currentPubkey}
          errorMessage={
            homeFeedQuery.error instanceof Error
              ? homeFeedQuery.error.message
              : undefined
          }
          feed={homeFeedQuery.data}
          isLoading={homeFeedQuery.isLoading}
          onOpenChannel={onOpenChannel}
          onRefresh={() => {
            void homeFeedQuery.refetch();
          }}
        />
      </div>
    </>
  );
}
