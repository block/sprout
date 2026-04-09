import * as React from "react";

import {
  useManagedAgentsQuery,
  useStopManagedAgentMutation,
} from "@/features/agents/hooks";
import {
  resolveUserLabel,
  type UserProfileLookup,
} from "@/features/profile/lib/identity";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import type { Channel, ManagedAgent } from "@/shared/api/types";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";

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

function formatElapsed(startIso: string): string {
  const startMs = new Date(startIso).getTime();
  const nowMs = Date.now();
  const totalSeconds = Math.max(0, Math.floor((nowMs - startMs) / 1000));

  if (totalSeconds >= 3600) {
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    return `${hours}h ${minutes}m`;
  }

  if (totalSeconds >= 60) {
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${minutes}m ${seconds}s`;
  }

  return `${totalSeconds}s`;
}

type BotTypingPopoverContentProps = {
  botAgents: ManagedAgent[];
};

function BotTypingPopoverContent({ botAgents }: BotTypingPopoverContentProps) {
  const [, setTick] = React.useState(0);
  const mutation = useStopManagedAgentMutation();

  React.useEffect(() => {
    const interval = setInterval(() => {
      setTick((prev) => prev + 1);
    }, 1000);
    return () => clearInterval(interval);
  }, []);

  function handleInterrupt() {
    for (const agent of botAgents) {
      mutation.mutate(agent.pubkey);
    }
  }

  return (
    <div>
      {botAgents.map((agent) => (
        <div key={agent.pubkey} className="mb-2 last:mb-0">
          <div className="font-bold text-sm">{agent.name}</div>
          {agent.model && (
            <div className="text-xs text-muted-foreground">{agent.model}</div>
          )}
          <div className="text-xs text-muted-foreground">
            {agent.lastStartedAt ? formatElapsed(agent.lastStartedAt) : "—"}
          </div>
        </div>
      ))}
      <button
        type="button"
        className="mt-3 w-full rounded-md bg-destructive px-3 py-1.5 text-sm font-medium text-destructive-foreground hover:bg-destructive/90 disabled:opacity-50"
        data-testid="typing-interrupt-button"
        disabled={mutation.isPending}
        onClick={handleInterrupt}
      >
        {mutation.isPending ? "Interrupting…" : "Interrupt"}
      </button>
    </div>
  );
}

export function TypingIndicatorRow({
  channel,
  currentPubkey,
  profiles,
  typingPubkeys,
}: TypingIndicatorRowProps) {
  const { data: managedAgents } = useManagedAgentsQuery();

  const managedAgentMap = React.useMemo(() => {
    const map = new Map<string, ManagedAgent>();
    if (managedAgents) {
      for (const agent of managedAgents) {
        map.set(agent.pubkey.toLowerCase(), agent);
      }
    }
    return map;
  }, [managedAgents]);

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

  const botAgents = React.useMemo(
    () =>
      typingPubkeys
        .map((pubkey) => managedAgentMap.get(pubkey.toLowerCase()))
        .filter((agent): agent is ManagedAgent => agent !== undefined),
    [typingPubkeys, managedAgentMap],
  );

  const hasBotTypers = botAgents.length > 0;

  if (labels.length === 0) {
    return null;
  }

  const typingText = formatTypingLabel(labels);

  return (
    <div
      aria-live="polite"
      className="bg-background/95 px-4 py-2 sm:px-6"
      data-testid="message-typing-indicator"
    >
      <div className="mx-auto flex w-full max-w-4xl items-center gap-2">
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
        {hasBotTypers ? (
          <Popover>
            <PopoverTrigger asChild>
              <button
                type="button"
                className="truncate text-sm text-muted-foreground cursor-pointer hover:text-foreground transition-colors"
                data-testid="message-typing-indicator-label"
              >
                {typingText}
              </button>
            </PopoverTrigger>
            <PopoverContent side="top" align="start" className="w-64 p-3">
              <BotTypingPopoverContent botAgents={botAgents} />
            </PopoverContent>
          </Popover>
        ) : (
          <p
            className="truncate text-sm text-muted-foreground"
            data-testid="message-typing-indicator-label"
          >
            {typingText}
          </p>
        )}
      </div>
    </div>
  );
}
