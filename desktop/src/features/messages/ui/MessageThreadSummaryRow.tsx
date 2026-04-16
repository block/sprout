import type {
  TimelineThreadSummary,
  TimelineThreadSummaryParticipant,
} from "@/features/messages/lib/threadPanel";
import type { TimelineMessage } from "@/features/messages/types";
import { UserAvatar } from "@/shared/ui/UserAvatar";

function ParticipantAvatar({
  participant,
  index,
}: {
  participant: TimelineThreadSummaryParticipant;
  index: number;
}) {
  return (
    <div className={index > 0 ? "-ml-2" : ""} style={{ zIndex: 10 - index }}>
      <UserAvatar
        avatarUrl={participant.avatarUrl}
        className="rounded-full border-2 border-background"
        displayName={participant.author}
        size="xs"
      />
    </div>
  );
}

export function MessageThreadSummaryRow({
  depth = 0,
  message,
  onOpenThread,
  summary,
}: {
  depth?: number;
  message: TimelineMessage;
  onOpenThread: (message: TimelineMessage) => void;
  summary: TimelineThreadSummary;
}) {
  const visibleDepth = Math.min(Math.max(depth, 0), 6);
  const marginLeftPx = visibleDepth * 28;
  const depthGuideOffsets = Array.from(
    { length: visibleDepth },
    (_, index) => 14 + index * 28,
  );

  return (
    <div className="relative">
      {depthGuideOffsets.length > 0 ? (
        <div
          aria-hidden
          className="pointer-events-none absolute left-0"
          style={{ bottom: "-4px", top: "-4px" }}
        >
          {depthGuideOffsets.map((offset, index) => (
            <div
              className="absolute bottom-0 top-0 border-l border-border/70"
              key={`${message.id}-summary-depth-guide-${offset}`}
              style={{
                left: `${offset}px`,
                opacity: index === depthGuideOffsets.length - 1 ? 0.9 : 0.55,
              }}
            />
          ))}
        </div>
      ) : null}

      <button
        className="flex w-fit max-w-full items-center gap-2 rounded-xl px-3 py-2 text-left text-sm text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
        data-thread-head-id={message.id}
        data-testid="message-thread-summary"
        onClick={() => onOpenThread(message)}
        style={{ marginLeft: `${marginLeftPx}px` }}
        type="button"
      >
        <div className="flex shrink-0 items-center">
          {summary.participants.map((participant, index) => (
            <ParticipantAvatar
              index={index}
              key={participant.id}
              participant={participant}
            />
          ))}
        </div>
        <div className="min-w-0">
          <div className="font-medium">
            <span>
              {summary.replyCount}{" "}
              {summary.replyCount === 1 ? "reply" : "replies"}
            </span>
          </div>
        </div>
      </button>
    </div>
  );
}
