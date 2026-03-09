import type { Message } from "@/features/chat/data/chatData";
import { cn } from "@/shared/lib/cn";
import { Separator } from "@/shared/ui/separator";

type MessageTimelineProps = {
  messages: Message[];
};

function MessageRow({ message }: { message: Message }) {
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

      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <h3 className="font-semibold tracking-tight">{message.author}</h3>
          <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground">
            {message.role}
          </p>
          <p className="text-sm text-muted-foreground">{message.time}</p>
        </div>
        <p className="max-w-3xl break-words text-sm leading-7 text-foreground/90">
          {message.body}
        </p>
      </div>
    </article>
  );
}

export function MessageTimeline({ messages }: MessageTimelineProps) {
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

        {messages.map((message) => (
          <MessageRow
            key={`${message.author}-${message.time}`}
            message={message}
          />
        ))}
      </div>
    </div>
  );
}
