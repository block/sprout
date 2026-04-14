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
  message,
  onOpenThread,
  summary,
}: {
  message: TimelineMessage;
  onOpenThread: (message: TimelineMessage) => void;
  summary: TimelineThreadSummary;
}) {
  return (
    <button
      className="ml-[3.75rem] flex w-fit max-w-full items-center gap-2 rounded-xl pl-0 pr-2 pt-0 pb-0.5 text-left text-sm text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
      data-thread-head-id={message.id}
      data-testid="message-thread-summary"
      onClick={() => onOpenThread(message)}
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
  );
}
