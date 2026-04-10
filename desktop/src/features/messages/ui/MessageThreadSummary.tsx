import * as React from "react";
import { MessageSquare } from "lucide-react";

import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import { cn } from "@/shared/lib/cn";

type MessageThreadSummaryProps = {
  message: TimelineMessage;
  onOpenThread?: (message: TimelineMessage) => void;
  profiles?: UserProfileLookup;
};

function resolveParticipantLabel(
  pubkey: string,
  profiles?: UserProfileLookup,
): string {
  const profile = profiles?.[pubkey.toLowerCase()];
  return (
    profile?.displayName?.trim() ||
    profile?.nip05Handle?.trim() ||
    `${pubkey.slice(0, 8)}...`
  );
}

export function MessageThreadSummary({
  message,
  onOpenThread,
  profiles,
}: MessageThreadSummaryProps) {
  const summary = message.threadSummary;
  const participants = React.useMemo(() => {
    if (!summary) {
      return [];
    }

    const seen = new Set<string>();
    const items: Array<{
      pubkey: string | null;
      label: string;
      avatarUrl: string | null;
    }> = [];

    if (message.pubkey) {
      const rootPubkey = message.pubkey.toLowerCase();
      seen.add(rootPubkey);
      items.push({
        pubkey: rootPubkey,
        label: message.author,
        avatarUrl: message.avatarUrl ?? null,
      });
    } else {
      items.push({
        pubkey: null,
        label: message.author,
        avatarUrl: message.avatarUrl ?? null,
      });
    }

    for (const pubkey of summary.participants) {
      const normalized = pubkey.toLowerCase();
      if (seen.has(normalized)) {
        continue;
      }

      seen.add(normalized);
      items.push({
        pubkey: normalized,
        label: resolveParticipantLabel(normalized, profiles),
        avatarUrl: profiles?.[normalized]?.avatarUrl ?? null,
      });
    }

    return items;
  }, [message.author, message.avatarUrl, message.pubkey, profiles, summary]);

  if (!summary || summary.descendantCount <= 0) {
    return null;
  }

  const visibleParticipants = participants.slice(0, 3);
  const overflowCount = participants.length - visibleParticipants.length;

  return (
    <button
      className={cn(
        "ml-12 mt-1 flex w-fit max-w-[calc(100%-3rem)] items-center gap-2 text-left text-xs font-semibold text-muted-foreground transition-colors hover:text-foreground",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
      )}
      data-testid={`message-thread-summary-${message.id}`}
      onClick={() => {
        onOpenThread?.(message);
      }}
      type="button"
    >
      <div className="flex items-center -space-x-1.5">
        {visibleParticipants.map((participant, index) => (
          <ProfileAvatar
            avatarUrl={participant.avatarUrl}
            className="h-5 w-5 rounded-full border border-background text-[8px]"
            key={participant.pubkey ?? `fallback-${index}`}
            label={participant.label}
          />
        ))}
        {overflowCount > 0 ? (
          <div className="flex h-5 w-5 items-center justify-center rounded-full border border-background bg-secondary text-[9px] font-semibold text-secondary-foreground shadow-sm">
            +{overflowCount}
          </div>
        ) : null}
      </div>
      <div className="flex min-w-0 items-center gap-1.5 truncate">
        <MessageSquare className="h-3.5 w-3.5 shrink-0" />
        <span className="truncate">
          {summary.descendantCount}{" "}
          {summary.descendantCount === 1 ? "reply" : "replies"}
        </span>
      </div>
    </button>
  );
}
