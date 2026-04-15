import * as React from "react";
import { Activity, AtSign, Bot, CircleAlert, RefreshCcw } from "lucide-react";

import { useRelayAgentsQuery } from "@/features/agents/hooks";
import { useFeedItemState } from "@/features/home/useFeedItemState";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import { useContactListQuery, useTimelineQuery } from "@/features/pulse/hooks";
import type { FeedItem, HomeFeedResponse } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { Skeleton } from "@/shared/ui/skeleton";

const FeedSection = React.lazy(async () => {
  const module = await import("./FeedSection");
  return { default: module.FeedSection };
});

const RecentNotesSection = React.lazy(async () => {
  const module = await import("./RecentNotesSection");
  return { default: module.RecentNotesSection };
});

type FeedFilter =
  | "all"
  | "mention"
  | "needs_action"
  | "activity"
  | "agent_activity";

function HomeLoadingState() {
  return (
    <div className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-3 sm:px-6">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
        <div className="grid gap-4 xl:grid-cols-2">
          {["mentions", "actions"].map((section) => (
            <div key={section}>
              <Skeleton className="mb-2 h-4 w-24" />
              <div className="space-y-0 rounded-md border border-border/60">
                {["a", "b", "c"].map((row) => (
                  <Skeleton className="h-16" key={row} />
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

const FILTER_OPTIONS: { value: FeedFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "mention", label: "Mentions" },
  { value: "needs_action", label: "Needs Action" },
  { value: "activity", label: "Activity" },
  { value: "agent_activity", label: "Agent Updates" },
];

type HomeViewProps = {
  feed?: HomeFeedResponse;
  isLoading?: boolean;
  errorMessage?: string;
  currentPubkey?: string;
  availableChannelIds: ReadonlySet<string>;
  onOpenFeedItem: (item: FeedItem) => void;
  onOpenPulse: () => void;
  onRefresh: () => void;
};

export function HomeView({
  feed,
  isLoading = false,
  errorMessage,
  currentPubkey,
  availableChannelIds,
  onOpenFeedItem,
  onOpenPulse,
  onRefresh,
}: HomeViewProps) {
  const [filter, setFilter] = React.useState<FeedFilter>("all");
  const { doneSet, markDone, undoDone } = useFeedItemState(currentPubkey);

  // Recent notes for the Pulse widget
  const contactListQuery = useContactListQuery(currentPubkey);
  const contactPubkeys = React.useMemo(
    () => (contactListQuery.data?.contacts ?? []).map((c) => c.pubkey),
    [contactListQuery.data],
  );
  const notesPubkeys = React.useMemo(
    () =>
      currentPubkey
        ? [...new Set([currentPubkey, ...contactPubkeys])]
        : contactPubkeys,
    [currentPubkey, contactPubkeys],
  );
  const notesTimelineQuery = useTimelineQuery(
    notesPubkeys,
    notesPubkeys.length > 0,
  );
  const recentNotes = notesTimelineQuery.data?.notes?.slice(0, 5) ?? [];
  const noteAuthorPubkeys = React.useMemo(
    () => [...new Set(recentNotes.map((n) => n.pubkey))],
    [recentNotes],
  );
  const noteProfilesQuery = useUsersBatchQuery(noteAuthorPubkeys, {
    enabled: noteAuthorPubkeys.length > 0,
  });
  const noteProfiles = noteProfilesQuery.data?.profiles ?? {};
  const relayAgentsQuery = useRelayAgentsQuery();
  const agentPubkeySet = React.useMemo(
    () => new Set((relayAgentsQuery.data ?? []).map((a) => a.pubkey)),
    [relayAgentsQuery.data],
  );

  const feedItems = feed
    ? [
        ...feed.feed.mentions,
        ...feed.feed.needsAction,
        ...(feed.feed.activity ?? []),
        ...(feed.feed.agentActivity ?? []),
      ]
    : [];
  const feedProfilesQuery = useUsersBatchQuery(
    feedItems.map((item) => item.pubkey),
    {
      enabled: feedItems.length > 0,
    },
  );
  const feedProfiles = feedProfilesQuery.data?.profiles;

  if (isLoading && !feed) {
    return <HomeLoadingState />;
  }

  if (!feed) {
    return (
      <div className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-3 sm:px-6">
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-4">
          <div className="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-5">
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

  const showMentions = filter === "all" || filter === "mention";
  const showNeedsAction = filter === "all" || filter === "needs_action";
  const showActivity = filter === "all" || filter === "activity";
  const showAgentActivity = filter === "all" || filter === "agent_activity";
  const singleColumn = filter !== "all";

  return (
    <div className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-3 sm:px-6">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
        <div className="flex items-center gap-1.5">
          {FILTER_OPTIONS.map((option) => (
            <Button
              key={option.value}
              onClick={() => setFilter(option.value)}
              size="sm"
              type="button"
              variant={filter === option.value ? "default" : "ghost"}
              className="h-7 px-2.5 text-xs"
            >
              {option.label}
            </Button>
          ))}
        </div>

        {recentNotes.length > 0 ? (
          <React.Suspense fallback={null}>
            <RecentNotesSection
              agentPubkeys={agentPubkeySet}
              notes={recentNotes}
              onOpenPulse={onOpenPulse}
              profiles={noteProfiles}
            />
          </React.Suspense>
        ) : null}

        <React.Suspense fallback={null}>
          <div className={`grid gap-5 ${singleColumn ? "" : "xl:grid-cols-2"}`}>
            {showMentions ? (
              <FeedSection
                availableChannelIds={availableChannelIds}
                currentPubkey={currentPubkey}
                profiles={feedProfiles}
                doneSet={doneSet}
                emptyDescription="When someone mentions you, it will land here."
                emptyTitle="No mentions right now"
                icon={AtSign}
                items={feed.feed.mentions}
                onMarkDone={markDone}
                onOpenItem={onOpenFeedItem}
                onUndoDone={undoDone}
                showDoneAction={false}
                title="Mentions"
              />
            ) : null}
            {showNeedsAction ? (
              <FeedSection
                availableChannelIds={availableChannelIds}
                currentPubkey={currentPubkey}
                profiles={feedProfiles}
                doneSet={doneSet}
                emptyDescription="Approval requests and reminders will appear here."
                emptyTitle="Nothing needs action"
                icon={CircleAlert}
                items={feed.feed.needsAction}
                onMarkDone={markDone}
                onOpenItem={onOpenFeedItem}
                onUndoDone={undoDone}
                showDoneAction={true}
                title="Needs Action"
              />
            ) : null}
            {showActivity ? (
              <FeedSection
                availableChannelIds={availableChannelIds}
                currentPubkey={currentPubkey}
                profiles={feedProfiles}
                doneSet={doneSet}
                emptyDescription="Recent channel messages and forum posts will show up here."
                emptyTitle="No channel activity yet"
                icon={Activity}
                items={feed.feed.activity ?? []}
                onMarkDone={markDone}
                onOpenItem={onOpenFeedItem}
                onUndoDone={undoDone}
                showDoneAction={false}
                title="Channel Activity"
              />
            ) : null}
            {showAgentActivity ? (
              <FeedSection
                availableChannelIds={availableChannelIds}
                currentPubkey={currentPubkey}
                profiles={feedProfiles}
                doneSet={doneSet}
                emptyDescription="Agent job requests, progress, and results will appear here."
                emptyTitle="No agent updates yet"
                icon={Bot}
                items={feed.feed.agentActivity ?? []}
                onMarkDone={markDone}
                onOpenItem={onOpenFeedItem}
                onUndoDone={undoDone}
                showDoneAction={false}
                title="Agent Updates"
              />
            ) : null}
          </div>
        </React.Suspense>
      </div>
    </div>
  );
}
