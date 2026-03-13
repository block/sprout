import { AtSign, CircleAlert, RefreshCcw, type LucideIcon } from "lucide-react";

import {
  resolveUserLabel,
  type UserProfileLookup,
} from "@/features/profile/lib/identity";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import type { FeedItem, HomeFeedResponse } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { Markdown } from "@/shared/ui/markdown";
import { Skeleton } from "@/shared/ui/skeleton";

const relativeTimeFormatter = new Intl.RelativeTimeFormat("en-US", {
  numeric: "auto",
});

function formatRelativeTime(unixSeconds: number) {
  const diff = unixSeconds - Math.floor(Date.now() / 1_000);
  const absoluteDiff = Math.abs(diff);

  if (absoluteDiff < 60) {
    return relativeTimeFormatter.format(diff, "second");
  }

  if (absoluteDiff < 60 * 60) {
    return relativeTimeFormatter.format(Math.round(diff / 60), "minute");
  }

  if (absoluteDiff < 60 * 60 * 24) {
    return relativeTimeFormatter.format(Math.round(diff / (60 * 60)), "hour");
  }

  if (absoluteDiff < 60 * 60 * 24 * 7) {
    return relativeTimeFormatter.format(
      Math.round(diff / (60 * 60 * 24)),
      "day",
    );
  }

  return new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(unixSeconds * 1_000));
}

function feedHeadline(item: FeedItem) {
  switch (item.kind) {
    case 40007:
      return "Reminder";
    case 43001:
      return "Job requested";
    case 43002:
      return "Job accepted";
    case 43003:
      return "Progress update";
    case 43004:
      return "Job result";
    case 43005:
      return "Job cancelled";
    case 43006:
      return "Job failed";
    case 45001:
      return "Forum post";
    case 45003:
      return "Forum reply";
    case 46010:
      return "Approval requested";
    default:
      if (item.category === "mention") {
        return "Mention";
      }

      if (item.category === "agent_activity") {
        return "Agent update";
      }

      return "Channel update";
  }
}

function feedContent(item: FeedItem) {
  const content = item.content.trim();
  if (content.length > 0) {
    return content;
  }

  if (item.kind === 46010) {
    return "A workflow is waiting for approval.";
  }

  if (item.kind === 40007) {
    return "A reminder is waiting for you.";
  }

  return "No additional details were attached to this event.";
}

type FeedSectionProps = {
  title: string;
  description: string;
  emptyTitle: string;
  emptyDescription: string;
  icon: LucideIcon;
  items: FeedItem[];
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  availableChannelIds: ReadonlySet<string>;
  onOpenChannel: (channelId: string) => void;
};

function FeedSection({
  title,
  description,
  emptyTitle,
  emptyDescription,
  icon: Icon,
  items,
  currentPubkey,
  profiles,
  availableChannelIds,
  onOpenChannel,
}: FeedSectionProps) {
  return (
    <section className="rounded-xl border border-border/80 bg-card/80 p-5 shadow-sm">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <Icon className="h-4 w-4" />
            </div>
            <div>
              <h2 className="text-base font-semibold tracking-tight">
                {title}
              </h2>
              <p className="text-sm text-muted-foreground">{description}</p>
            </div>
          </div>
        </div>
        <div className="rounded-full bg-muted px-3 py-1 text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
          {items.length}
        </div>
      </div>

      <div className="mt-5 space-y-3">
        {items.length === 0 ? (
          <div className="rounded-lg border border-dashed border-border/80 bg-background/60 px-5 py-7 text-center">
            <p className="text-sm font-semibold tracking-tight">{emptyTitle}</p>
            <p className="mt-2 text-sm text-muted-foreground">
              {emptyDescription}
            </p>
          </div>
        ) : null}

        {items.map((item) => {
          const channelId = item.channelId;
          const canOpenChannel =
            channelId !== null && availableChannelIds.has(channelId);

          return (
            <article
              className="rounded-lg border border-border/70 bg-background/70 p-4 shadow-sm"
              key={item.id}
            >
              <div className="flex gap-3">
                <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-secondary/70 text-secondary-foreground">
                  <Icon className="h-4 w-4" />
                </div>

                <div className="min-w-0 flex-1">
                  <div className="flex min-w-0 flex-wrap items-center gap-2">
                    <h3 className="text-sm font-semibold tracking-tight">
                      {feedHeadline(item)}
                    </h3>
                    <p className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
                      {resolveUserLabel({
                        pubkey: item.pubkey,
                        currentPubkey,
                        profiles,
                        preferResolvedSelfLabel: true,
                      })}
                    </p>
                    {item.channelName ? (
                      <p className="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.16em] text-primary">
                        {item.channelName}
                      </p>
                    ) : null}
                    <p className="ml-auto whitespace-nowrap text-xs text-muted-foreground">
                      {formatRelativeTime(item.createdAt)}
                    </p>
                  </div>

                  <Markdown
                    className="mt-2 max-w-none"
                    compact
                    content={feedContent(item)}
                  />
                </div>

                {canOpenChannel ? (
                  <div className="hidden shrink-0 sm:block">
                    <Button
                      onClick={() => {
                        if (channelId) {
                          onOpenChannel(channelId);
                        }
                      }}
                      size="sm"
                      type="button"
                      variant="outline"
                    >
                      Open
                    </Button>
                  </div>
                ) : null}
              </div>

              {canOpenChannel ? (
                <div className="mt-3 sm:hidden">
                  <Button
                    className="w-full"
                    onClick={() => {
                      if (channelId) {
                        onOpenChannel(channelId);
                      }
                    }}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    Open channel
                  </Button>
                </div>
              ) : null}
            </article>
          );
        })}
      </div>
    </section>
  );
}

function HomeLoadingState() {
  return (
    <div className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-4 sm:px-6">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-6">
        <div className="rounded-xl border border-border/80 bg-card/80 p-5 shadow-sm">
          <Skeleton className="h-6 w-32" />
          <Skeleton className="mt-3 h-4 w-full max-w-lg" />
        </div>

        <div className="grid gap-4 xl:grid-cols-2">
          {["mentions", "actions"].map((section) => (
            <div
              className="rounded-xl border border-border/80 bg-card/80 p-5 shadow-sm"
              key={section}
            >
              <Skeleton className="h-6 w-32" />
              <Skeleton className="mt-3 h-4 w-full max-w-xs" />
              <div className="mt-5 space-y-3">
                {["a", "b", "c"].map((row) => (
                  <Skeleton className="h-28 rounded-lg" key={row} />
                ))}
              </div>
            </div>
          ))}
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
  const feedItems = feed
    ? [...feed.feed.mentions, ...feed.feed.needsAction]
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
      <div className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-4 sm:px-6">
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-4">
          <div className="rounded-xl border border-destructive/30 bg-destructive/5 px-6 py-8 shadow-sm">
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

  return (
    <div className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-4 sm:px-6">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-6">
        <div className="grid gap-4 xl:grid-cols-2">
          <FeedSection
            availableChannelIds={availableChannelIds}
            currentPubkey={currentPubkey}
            profiles={feedProfiles}
            description="Messages where your pubkey was tagged."
            emptyDescription="When someone mentions you in an accessible channel, it will land here."
            emptyTitle="No mentions right now"
            icon={AtSign}
            items={feed.feed.mentions}
            onOpenChannel={onOpenChannel}
            title="@Mentions"
          />
          <FeedSection
            availableChannelIds={availableChannelIds}
            currentPubkey={currentPubkey}
            profiles={feedProfiles}
            description="Approvals and reminders that need you."
            emptyDescription="Workflow approval requests and reminders will appear here."
            emptyTitle="Nothing needs action"
            icon={CircleAlert}
            items={feed.feed.needsAction}
            onOpenChannel={onOpenChannel}
            title="Needs Action"
          />
        </div>
      </div>
    </div>
  );
}
