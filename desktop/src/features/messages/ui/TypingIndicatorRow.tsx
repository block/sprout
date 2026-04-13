import * as React from "react";

import {
  resolveUserLabel,
  type UserProfileLookup,
} from "@/features/profile/lib/identity";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import type { Channel } from "@/shared/api/types";

type TypingIndicatorRowProps = {
  channel: Channel | null;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  typingPubkeys: string[];
};

function resolveFallbackName(channel: Channel | null, pubkey: string) {
  if (!channel || channel.channelType !== "dm") {
    return null;
  }

  const participantIndex = channel.participantPubkeys.findIndex(
    (candidate) => candidate.toLowerCase() === pubkey.toLowerCase(),
  );

  if (participantIndex < 0) {
    return null;
  }

  return channel.participants[participantIndex] ?? null;
}

function formatTypingLabel(names: string[]) {
  if (names.length === 1) {
    return `${names[0]} is typing...`;
  }

  if (names.length === 2) {
    return `${names[0]} and ${names[1]} are typing...`;
  }

  if (names.length === 3) {
    return `${names[0]}, ${names[1]}, and ${names[2]} are typing...`;
  }

  return `${names[0]}, ${names[1]}, and ${names.length - 2} others are typing...`;
}

export function TypingIndicatorRow({
  channel,
  currentPubkey,
  profiles,
  typingPubkeys,
}: TypingIndicatorRowProps) {
  const labels = React.useMemo(
    () =>
      typingPubkeys.map((pubkey) =>
        resolveUserLabel({
          pubkey,
          currentPubkey,
          fallbackName: resolveFallbackName(channel, pubkey),
          profiles,
          preferResolvedSelfLabel: true,
        }),
      ),
    [channel, currentPubkey, profiles, typingPubkeys],
  );

  if (labels.length === 0) {
    return null;
  }

  return (
    <div
      aria-live="polite"
      className="bg-background/95 px-4 py-2 sm:px-6"
      data-testid="message-typing-indicator"
    >
      <div className="flex w-full items-center gap-2">
        <div className="flex flex-shrink-0 items-center">
          {typingPubkeys.map((pubkey, index) => {
            const profile = profiles?.[pubkey];
            const label = labels[index] ?? pubkey.slice(0, 8);
            return (
              <div
                key={pubkey}
                className={`relative h-5 w-5 flex-shrink-0 rounded-full ring-1 ring-background${index > 0 ? " -ml-1.5" : ""}`}
                data-testid="message-typing-avatar"
              >
                <ProfileAvatar
                  avatarUrl={profile?.avatarUrl ?? null}
                  label={label}
                  className="h-5 w-5 rounded-full text-[8px]"
                  iconClassName="h-3 w-3"
                />
              </div>
            );
          })}
        </div>
        <p
          className="truncate text-sm text-muted-foreground"
          data-testid="message-typing-indicator-label"
        >
          {formatTypingLabel(labels)}
        </p>
      </div>
    </div>
  );
}
