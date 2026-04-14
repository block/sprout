import * as React from "react";
import { RefreshCcw } from "lucide-react";

import {
  type InboxFilter,
  type InboxReply,
  buildInboxItems,
  formatInboxFullTimestamp,
} from "@/features/home/lib/inbox";
import { useFeedItemState } from "@/features/home/useFeedItemState";
import { InboxDetailPane } from "@/features/home/ui/InboxDetailPane";
import { InboxListPane } from "@/features/home/ui/InboxListPane";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import { resolveUserLabel } from "@/features/profile/lib/identity";
import { deleteMessage, sendChannelMessage } from "@/shared/api/tauri";
import type { HomeFeedResponse } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { Skeleton } from "@/shared/ui/skeleton";

function HomeLoadingState() {
  return (
    <div className="flex-1 overflow-hidden">
      <div className="grid h-full min-h-0 w-full lg:grid-cols-[320px_minmax(0,1fr)]">
        <div className="overflow-hidden border-r border-border/70 bg-background">
          <div className="border-b border-border/70 px-4 py-4">
            <Skeleton className="h-4 w-20" />
            <Skeleton className="mt-2 h-4 w-28" />
            <Skeleton className="mt-4 h-10 rounded-md" />
          </div>
          <div className="space-y-3 px-4 py-4">
            {["a", "b", "c", "d"].map((row) => (
              <Skeleton className="h-20 rounded-md" key={row} />
            ))}
          </div>
        </div>

        <div className="overflow-hidden bg-background">
          <div className="border-b border-border/70 px-5 py-4">
            <Skeleton className="h-5 w-48" />
            <Skeleton className="mt-3 h-8 w-72" />
          </div>
          <div className="px-5 py-5">
            <Skeleton className="h-64 rounded-md" />
          </div>
        </div>
      </div>
    </div>
  );
}

type HomeViewProps = {
  feed?: HomeFeedResponse;
  isLoading?: boolean;
  errorMessage?: string;
  currentPubkey?: string;
  availableChannelIds: ReadonlySet<string>;
  onOpenChannel: (channelId: string) => void;
  onRefresh: () => void;
};

export function HomeView({
  feed,
  isLoading = false,
  errorMessage,
  currentPubkey,
  availableChannelIds,
  onOpenChannel,
  onRefresh,
}: HomeViewProps) {
  const [filter, setFilter] = React.useState<InboxFilter>("all");
  const [searchValue, setSearchValue] = React.useState("");
  const [selectedItemId, setSelectedItemId] = React.useState<string | null>(null);
  const [isDeletingMessage, setIsDeletingMessage] = React.useState(false);
  const [isSendingReply, setIsSendingReply] = React.useState(false);
  const [localRepliesByItemId, setLocalRepliesByItemId] = React.useState<
    Record<string, InboxReply[]>
  >({});
  const { doneSet, markDone, undoDone } = useFeedItemState(currentPubkey);
  const feedItems = React.useMemo(
    () => (feed ? [...feed.feed.mentions, ...feed.feed.needsAction] : []),
    [feed],
  );
  const feedProfilePubkeys = React.useMemo(
    () =>
      [
        ...new Set([
          ...feedItems.map((item) => item.pubkey),
          ...(currentPubkey ? [currentPubkey] : []),
        ]),
      ],
    [currentPubkey, feedItems],
  );
  const feedProfilesQuery = useUsersBatchQuery(
    feedProfilePubkeys,
    {
      enabled: feedProfilePubkeys.length > 0,
    },
  );
  const feedProfiles = feedProfilesQuery.data?.profiles;
  const inboxItems = React.useMemo(
    () =>
      buildInboxItems({
        currentPubkey,
        feed,
        profiles: feedProfiles,
      }),
    [currentPubkey, feed, feedProfiles],
  );
  const filteredItems = React.useMemo(() => {
    const normalizedQuery = searchValue.trim().toLowerCase();

    return inboxItems.filter((item) => {
      const matchesFilter =
        filter === "all" ? true : item.item.category === filter;
      const matchesQuery =
        normalizedQuery.length === 0 ||
        item.searchableText.includes(normalizedQuery);

      return matchesFilter && matchesQuery;
    });
  }, [filter, inboxItems, searchValue]);
  const selectedItem =
    filteredItems.find((item) => item.id === selectedItemId) ?? null;
  const selectedItemReplies = selectedItem
    ? localRepliesByItemId[selectedItem.id] ?? []
    : [];
  React.useEffect(() => {
    if (filteredItems.length === 0) {
      setSelectedItemId(null);
      return;
    }

    if (!filteredItems.some((item) => item.id === selectedItemId)) {
      setSelectedItemId(filteredItems[0]?.id ?? null);
    }
  }, [filteredItems, selectedItemId]);

  React.useEffect(() => {
    setIsDeletingMessage(false);
    setIsSendingReply(false);
  }, [selectedItemId]);

  const handleToggleDone = React.useCallback(
    (itemId: string) => {
      if (doneSet.has(itemId)) {
        undoDone(itemId);
        return;
      }

      markDone(itemId);
    },
    [doneSet, markDone, undoDone],
  );

  if (isLoading && !feed) {
    return <HomeLoadingState />;
  }

  if (!feed) {
    return (
      <div className="flex-1 overflow-hidden px-4 py-3 sm:px-6">
        <div className="flex w-full max-w-3xl flex-col gap-4">
          <div className="border border-destructive/30 bg-destructive/5 px-5 py-6">
            <p className="text-base font-semibold tracking-tight">
              Home feed unavailable
            </p>
            <p className="mt-2 text-sm text-muted-foreground">
              {errorMessage ?? "The relay did not return a feed response."}
            </p>
            <Button className="mt-5" onClick={onRefresh} type="button">
              <RefreshCcw className="h-4 w-4" />
              Try again
            </Button>
          </div>
        </div>
      </div>
    );
  }

  const canReply =
    selectedItem !== null &&
    selectedItem.item.channelId !== null &&
    availableChannelIds.has(selectedItem.item.channelId) &&
    selectedItem.item.kind !== 45001 &&
    selectedItem.item.kind !== 45003;
  const disabledReplyReason =
    canReply || !selectedItem
      ? null
      : selectedItem.item.channelId
        ? availableChannelIds.has(selectedItem.item.channelId)
          ? "This item does not support inline replies yet."
          : "Open the linked channel to reply."
        : "This inbox item does not have a reply target.";
  const canDelete =
    selectedItem !== null &&
    currentPubkey?.trim().toLowerCase() ===
      selectedItem.item.pubkey.trim().toLowerCase();

  return (
    <div className="flex-1 overflow-hidden">
      <div
        className="grid h-full min-h-0 w-full lg:grid-cols-[320px_minmax(0,1fr)]"
        data-testid="home-inbox"
      >
        <InboxListPane
          doneSet={doneSet}
          filter={filter}
          items={filteredItems}
          onFilterChange={setFilter}
          onSearchChange={setSearchValue}
          onSelect={setSelectedItemId}
          searchValue={searchValue}
          selectedId={selectedItemId}
        />

        <InboxDetailPane
          canDelete={canDelete}
          canOpenChannel={Boolean(
            selectedItem?.item.channelId &&
              availableChannelIds.has(selectedItem.item.channelId),
          )}
          canReply={canReply}
          disabledReplyReason={disabledReplyReason}
          isDone={selectedItem ? doneSet.has(selectedItem.id) : false}
          isDeletingMessage={isDeletingMessage}
          isSendingReply={isSendingReply}
          item={selectedItem}
          localReplies={selectedItemReplies}
          onArchive={() => {
            if (selectedItem) {
              handleToggleDone(selectedItem.id);
            }
          }}
          onDelete={() => {
            if (!selectedItem || !canDelete) {
              return;
            }

            setIsDeletingMessage(true);
            void deleteMessage(selectedItem.id)
              .then(() => {
                onRefresh();
              })
              .finally(() => {
                setIsDeletingMessage(false);
              });
          }}
          onOpenChannel={onOpenChannel}
          onSendReply={async (content, mentionPubkeys, mediaTags) => {
            const channelId = selectedItem?.item.channelId;
            if (!selectedItem || !channelId || !canReply) {
              throw new Error("Replies are not available for this item.");
            }

            const itemToReply = selectedItem;
            setIsSendingReply(true);
            try {
              const result = await sendChannelMessage(
                channelId,
                content,
                itemToReply.id,
                mediaTags,
                mentionPubkeys,
              );
              const authorPubkey = currentPubkey ?? itemToReply.item.pubkey;
              const reply: InboxReply = {
                authorLabel: currentPubkey
                  ? resolveUserLabel({
                      currentPubkey,
                      profiles: feedProfiles,
                      pubkey: authorPubkey,
                    })
                  : "You",
                avatarUrl:
                  currentPubkey && feedProfiles
                    ? (feedProfiles[currentPubkey.trim().toLowerCase()]?.avatarUrl ??
                      null)
                    : null,
                content,
                fullTimestampLabel: formatInboxFullTimestamp(result.createdAt),
                id: result.eventId,
              };
              setLocalRepliesByItemId((current) => ({
                ...current,
                [itemToReply.id]: [...(current[itemToReply.id] ?? []), reply],
              }));
              onRefresh();
            } finally {
              setIsSendingReply(false);
            }
          }}
          onToggleDone={() => {
            if (selectedItem) {
              handleToggleDone(selectedItem.id);
            }
          }}
        />
      </div>
    </div>
  );
}
