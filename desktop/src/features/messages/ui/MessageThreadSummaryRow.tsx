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
    <div
      className={index > 0 ? "-ml-2" : ""}
      data-testid="message-thread-summary-participant"
      style={{ zIndex: 10 - index }}
    >
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
  alignWithText = true,
  depth = 0,
  message,
  onOpenThread,
  summary,
  textColumnOffsetPx = 60,
}: {
  alignWithText?: boolean;
  depth?: number;
  message: TimelineMessage;
  onOpenThread: (message: TimelineMessage) => void;
  summary: TimelineThreadSummary;
  textColumnOffsetPx?: number;
}) {
  const visibleDepth = Math.min(Math.max(depth, 0), 6);
  const marginLeftPx = visibleDepth * 28 + textColumnOffsetPx;
  const depthGuideOffsets = Array.from(
    { length: visibleDepth },
    (_, index) => 14 + index * 28,
  );

  return (
    <div className="relative pb-2">
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
        className="inline-flex w-fit max-w-full items-center gap-1.5 rounded-full border border-border/60 bg-background px-2 py-1 text-left text-xs font-medium text-muted-foreground transition-colors hover:border-primary/30 hover:bg-primary/5 hover:text-foreground"
        data-thread-head-id={message.id}
        data-testid="message-thread-summary"
        onClick={() => onOpenThread(message)}
        style={alignWithText ? { marginLeft: `${marginLeftPx}px` } : undefined}
        type="button"
      >
        <span className="flex shrink-0 items-center">
          {summary.participants.map((participant, index) => (
            <ParticipantAvatar
              index={index}
              key={participant.id}
              participant={participant}
            />
          ))}
        </span>
        <span>
          {summary.replyCount} {summary.replyCount === 1 ? "reply" : "replies"}
        </span>
      </button>
    </div>
  );
}
