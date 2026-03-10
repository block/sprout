import type { TimelineMessage } from "@/features/messages/types";
import { cn } from "@/shared/lib/cn";
import { Markdown } from "@/shared/ui/markdown";
import { Separator } from "@/shared/ui/separator";
import { Skeleton } from "@/shared/ui/skeleton";

type MessageTimelineProps = {
  messages: TimelineMessage[];
  isLoading?: boolean;
  emptyTitle?: string;
  emptyDescription?: string;
};

function MessageRow({ message }: { message: TimelineMessage }) {
  const initials = message.author
    .split(" ")
    .map((part) => part[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();

  return (
    <article className="flex gap-3" data-testid="message-row">
      <div
        className={cn(
          "flex h-9 w-9 shrink-0 items-center justify-center rounded-xl text-xs font-semibold shadow-sm",
          message.accent
            ? "bg-primary text-primary-foreground"
            : "bg-secondary text-secondary-foreground",
        )}
      >
        {initials}
      </div>

      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h3 className="truncate text-sm font-semibold tracking-tight">
            {message.author}
          </h3>
          {message.role ? (
            <p className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
              {message.role}
            </p>
          ) : null}
          <div className="ml-auto flex items-center gap-2 text-xs text-muted-foreground">
            {message.pending ? (
              <p className="font-medium uppercase tracking-[0.14em] text-primary/80">
                Sending
              </p>
            ) : null}
            <p className="whitespace-nowrap">{message.time}</p>
          </div>
        </div>
        <Markdown className="max-w-3xl" compact content={message.body} />
      </div>
    </article>
  );
}

function TimelineSkeleton() {
  const skeletonRows = ["first", "second", "third", "fourth"];

  return (
    <>
      {skeletonRows.map((row) => (
        <div className="flex gap-3" key={row}>
          <Skeleton className="h-9 w-9 rounded-xl" />
          <div className="min-w-0 flex-1 space-y-1.5">
            <Skeleton className="h-3.5 w-44" />
            <Skeleton className="h-4 w-full max-w-2xl" />
            <Skeleton className="h-4 w-full max-w-xl" />
          </div>
        </div>
      ))}
    </>
  );
}

export function MessageTimeline({
  messages,
  isLoading = false,
  emptyTitle = "No messages yet",
  emptyDescription = "Send the first message to start the thread.",
}: MessageTimelineProps) {
  return (
    <div
      className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-4 sm:px-6"
      data-testid="message-timeline"
    >
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-4">
        <div className="flex items-center gap-4">
          <Separator className="flex-1" />
          <p className="text-xs font-semibold uppercase tracking-[0.22em] text-muted-foreground">
            Today
          </p>
          <Separator className="flex-1" />
        </div>

        {isLoading ? <TimelineSkeleton /> : null}

        {!isLoading && messages.length === 0 ? (
          <div
            className="rounded-3xl border border-dashed border-border/80 bg-card/70 px-6 py-10 text-center shadow-sm"
            data-testid="message-empty"
          >
            <p className="text-base font-semibold tracking-tight">
              {emptyTitle}
            </p>
            <p className="mt-2 text-sm text-muted-foreground">
              {emptyDescription}
            </p>
          </div>
        ) : null}

        {!isLoading
          ? messages.map((message) => (
              <MessageRow key={message.id} message={message} />
            ))
          : null}
      </div>
    </div>
  );
}
