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
    <article className="flex gap-4">
      <div
        className={cn(
          "flex h-11 w-11 shrink-0 items-center justify-center rounded-2xl text-sm font-semibold shadow-sm",
          message.accent
            ? "bg-primary text-primary-foreground"
            : "bg-secondary text-secondary-foreground",
        )}
      >
        {initials}
      </div>

      <div className="min-w-0 flex-1 space-y-2">
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <h3 className="font-semibold tracking-tight">{message.author}</h3>
          <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground">
            {message.role}
          </p>
          <p className="text-sm text-muted-foreground">{message.time}</p>
          {message.pending ? (
            <p className="text-xs font-medium uppercase tracking-[0.2em] text-primary/80">
              Sending
            </p>
          ) : null}
        </div>
        <Markdown className="max-w-3xl" content={message.body} />
      </div>
    </article>
  );
}

function TimelineSkeleton() {
  const skeletonRows = ["first", "second", "third", "fourth"];

  return (
    <>
      {skeletonRows.map((row) => (
        <div className="flex gap-4" key={row}>
          <Skeleton className="h-11 w-11 rounded-2xl" />
          <div className="min-w-0 flex-1 space-y-2">
            <Skeleton className="h-4 w-40" />
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
    <div className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-6 sm:px-6">
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-6">
        <div className="flex items-center gap-4">
          <Separator className="flex-1" />
          <p className="text-xs font-semibold uppercase tracking-[0.22em] text-muted-foreground">
            Today
          </p>
          <Separator className="flex-1" />
        </div>

        {isLoading ? <TimelineSkeleton /> : null}

        {!isLoading && messages.length === 0 ? (
          <div className="rounded-3xl border border-dashed border-border/80 bg-card/70 px-6 py-10 text-center shadow-sm">
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
